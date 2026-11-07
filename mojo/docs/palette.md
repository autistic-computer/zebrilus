# Palette model

The local palette opens in the source places/actions mode. The initial attached-host chip is automatic. A leading colon selects raw commands without dropping the place chips. Enter activates places or fills a verb slot; Tab enters a place. Empty-field Backspace discards a filled slot and clears the field; an automatically filled target discards the draft. Shift-Tab can return a popped chip for highlighting in its parent list.

`PaletteFleet` retains the host/session/window/pane hierarchy independently of terminal cell planes. `palette_candidates`, `palette_children`, and `search_places` preserve native scope, numeric query, offline-host, fuzzy-score, recency-boost and stable-tie rules. Written places use the actual `@host:%pane`, `@host:@window`, `@host:$session` and path/line suffix grammar.

`palette_verbs` retains curated actions, the command catalog, GUI offers and aliases in source order. Its alias tokenizer is the native completion tokenizer, including its partial quote behavior. Open alias slots are inferred by the actual option parser contract. `compile_palette_verb` preserves structured arguments when the source can do so, and returns command text otherwise. Raw execution uses `if-shell -F 1 <line>` so the daemon parses commands in its own context.

`PaletteController` owns query/cursor, chips, draft values and labels, selected row, navigation engagement, finishing state and the twenty-command transient history. Its effects request execution, activation, connection, notification or an external GUI/file level. Activation closes only after the host adapter reports success. `PaletteModel` builds local rows from the fleet and explicit `PaletteDecorations`; those decorations keep presentation metadata distinct from retained terminal content.

`PaletteRow` exposes label, detail, right text, UTF-8 match ranges, muted byte prefix, status, running state, icon name, indentation, expansion and an optional typed shortcut. Action tags are 0 header, 1 place, 2 verb, 3 command, 4 history, 5 target-slot fill and 6 text-slot submission. Pill kinds match the native Mode, Status, Code, Crumb and Action variants. Status values are Online 0, Running 1, Waiting 2 and Offline 3.

The model foundation does not yet activate application chrome. Agent chat summaries, decayed/persisted recents, argument completion and GUI/file levels require host services; the window/backend presentation adapter is subsequent integration work. Snapshot projection and private daemon controller execution are now exercised directly. Typed daemon prompt and chooser state is already available separately through `AppModel.dialogs`.

## Checks

All commands enter the shared 4 GiB build guard and run one job at a time:

```
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_places_vectors.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_verbs_vectors.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_controller.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_model_vectors.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_shortcut_vectors.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_projection_vectors.py
python3 scripts/port/guard.py pixi run --manifest-path mojo/pixi.toml python3 mojo/scripts/palette_live_smoke.py
```

The place differential compares 9,067 cases and 91,463 bytes against actual `zz-client` helpers. The verb differential compares 12,331 cases and 4,410,156 bytes against actual `zz-client` verbs and completion. The controller probe checks source-derived transitions and exact limits. The presentation differential compares 2,890 cases and 1,524,161 exact bytes. The presentation oracle calls the real `UnifiedPalette.rebuild_places`, retaining actual row, pill, placeholder, action and initial-selection outputs rather than a second implementation of its rules.

Interactive input follows the native 64 KiB limit. Pure search/tokenizer helpers accept at most 1 MiB of query or alias text. Fleets accept 65,536 hierarchy nodes and 16 MiB of text; the exact next-node/next-byte rejection is tested. Alias tokenization accepts 4,096 words, and presentation rows are capped at 65,536 with 16 MiB of aggregate text. A repeated 1 MiB ancestor-label regression verifies rejection while building the needs-you rows. GUI offers also have a 16 MiB aggregate text limit. These extra adapter refusals are explicit resource limits.

Target and GUI metadata are generated from source by the corresponding vector script with `--generate`; ordinary checks reject stale generated metadata. Reference artifacts and exact output reports are written under `target/port`.

Shortcuts retain the source Keys-versus-Text distinction. A missing shortcut is distinct from Text with an empty value. Keystrokes preserve `key`, optional `key_char`, and Shift/Ctrl/Alt/Platform/Function bits 1/2/4/8/16. The native source helper rejects named function keys such as `F1`; one invalid word makes the entire original spelling a Text shortcut. The actual source differential compares 4,672 cases and 149,646 exact bytes, including uppercase, minus, Unicode, whitespace, aliases and a 65,535-byte modifier chain. Prefix scanning is linear. Input text is capped at 64 KiB and sequences at 256 keys, with exact acceptance and next-bound refusal checks. Populated rows are checked for absent, empty Text, parsed Keys and full Text fallback, while the existing source presentation corpus remains exact.

Snapshot projection calls the source navigation rules for pane order and saturating indexes, empty/numeric session labels and cwd/repo tie breaking, all pane-kind labels, and detected programs. The generated program catalog retains all 154 entries. A 2,080-case source differential checks 67,842 exact bytes through real encoded/decoded snapshots and the projected fleet. Program matching preserves first candidate, 128-byte tail normalization, path boundaries, longest keys and extension fallback. Projection copies metadata without terminal planes, rejects beyond 65,536 combined nodes or 16 MiB of combined output text, and tests exact acceptance plus next-node/next-byte refusal. The process helper accepts 4,096 candidates with at most 1 MiB each; the exact next-count/next-text refusals are tested.

The private daemon check uses the native interactive stream and compiled Bend gate to exercise curated split targeting, quoted rename slots, daemon parsing of raw commands, published dynamic aliases and their inferred slots, structured/raw alias execution, and native window activation. All mutations remain outputless, so they leave command output closed. This is a controller/runtime check; it does not claim palette window rendering. Agent attention is supplied as optional metadata with source values Idle 0, Working 1, NeedsInput 2 and Failed 3. Custom agent Asset icon names are retained verbatim for future app-owned asset registration.
