# Buffer chooser

`zzmojo.buffer_presentation` retains the authoritative buffer snapshot and presents its rows without changing the selected source index. Custom row text replaces the name and clears the preview, size and age columns. Shortcut labels, key gutter detection, modal height, subtitle, filter status and search composition follow the native chooser.

Size formatting preserves the source's saturating unsigned arithmetic and decimal rounding. Age formatting accepts an explicit Unix timestamp and saturates future creation times to zero. The native GUI delegates its row formatting to the same Rust helper used by the differential oracle; it reads the clock only for ordinary rows.

Run `python3 mojo/scripts/buffer_presentation_vectors.py` from the repository root. The guarded check compares 4,615 numeric cases, 96 retained states and 1,488 rows against the Rust helper, including maximum unsigned values, unit and age boundaries, custom text, empty shortcuts, no matches and composed search text. The comparison covers 289,541 exact bytes. It verifies presentation behavior; it does not activate a live Buffer chooser controller.

`measure_buffer_chooser` and `render_buffer_chooser` use the shared modal frame with a 640px maximum width and 40px rows. The renderer keeps the Buffer snapshot and source indices intact. It accepts an explicit Unix timestamp, bounded scroll and marked search text; `overlay=False` paints inside an existing managed modal layer. Its `ChooserResult` separates selection, activation and dismissal and includes the panel and list viewport bounds.

Run `python3 mojo/scripts/buffer_scene_pixels.py`. All 126 actual native Buffer modal scenes match 83,349,000 RGBA pixels exactly across six themes, scales 1, 1.25 and 2, and seven ordinary, selected, searching, empty, narrow and notice-heavy cases. Maximum unsigned sizes exercise the native two-line numeric cell. Wrapped trailing spaces count in its right alignment; numeric text uses checked native records and physical ancestor origins. The small solid primary badge and the uppercase delete keycap follow the native widgets.

The comparison uses the closed Inter/JetBrains Mono pool and captured timestamps. It covers the widget's exercised presentation, independently of live chooser session, socket and input ownership. The desktop's native chooser capability remains gated until its complete controllers are integrated.
