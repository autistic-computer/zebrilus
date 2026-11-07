# Buffer chooser

`zzmojo.buffer_presentation` retains the authoritative buffer snapshot and presents its rows without changing the selected source index. Custom row text replaces the name and clears the preview, size and age columns. Shortcut labels, key gutter detection, modal height, subtitle, filter status and search composition follow the native chooser.

Size formatting preserves the source's saturating unsigned arithmetic and decimal rounding. Age formatting accepts an explicit Unix timestamp and saturates future creation times to zero. The native GUI delegates its row formatting to the same Rust helper used by the differential oracle; it reads the clock only for ordinary rows.

Run `python3 mojo/scripts/buffer_presentation_vectors.py` from the repository root. The guarded check compares 4,615 numeric cases, 96 retained states and 1,488 rows against the Rust helper, including maximum unsigned values, unit and age boundaries, custom text, empty shortcuts, no matches and composed search text. The comparison covers 289,541 exact bytes. It verifies presentation behavior; it does not activate a live Buffer chooser controller.
