`terminal_styled_fixture` paints the actual native terminal `RowRenderCache` through its GPU. Its window receives the full application font request from `terminal_font_for_style`, including features and explicit fallback families. An isolated font system loads only the manifest's files. The separate Mojo probe reads the same cells, packed styles, grapheme dictionary, overlays, cursor, appearance and font files. Its four font slots remain distinct even when they reference the same regular face. The existing terminal pixel baseline is unchanged.

Run `/usr/bin/python3 scripts/terminal_styled_pixels.py --record --scale 1 --scale 1.25 --scale 2` from `mojo` to capture all 24 comparisons under the shared 4 GiB guard. The script records native and Mojo PNGs, amplified differences, source quads/glyphs/underlines, candidate glyph bounds, exact metric bits and actual font/glyph diagnostics under `target/port/terminal-styled-reference`. Without `--record`, any differing pixel or base metric fails. `--native-only` captures the source without compiling the Mojo UI. Cases and scales can be selected individually.

At 13 logical pixels and device scales 1, 1.25 and 2, all twelve font cases match every pixel and base metric: 1,612,800 exact pixels covering regular-only fonts, all real style faces, regular plus bold with missing italic, and synthesis disabled. Each contains ordinary and selected regular, bold, italic and bold-italic text with ligatures. Fonts are JetBrains Mono Regular and the four installed DejaVu Sans Mono faces.

The other corrected-reference cases currently have these exact failure counts:

| Case | Scale 1 | Scale 1.25 | Scale 2 |
|---|---:|---:|---:|
| Decorations | 1,961 | 1,582 | 4,624 |
| Contrast | 1,961 | 1,582 | 4,624 |
| Multilingual glyphs | 344 | 495 | 670 |
| IME composition | 1,480 | 2,232 | 5,560 |

All 168 base metric bit patterns agree across the 24 cases. The remaining counts are recorded failures, not accepted tolerances. At scale 1, every multilingual text row outside box/block graphics is exact, including Greek, Cyrillic, Hebrew, Arabic, combining marks, Devanagari, Thai, CJK, wide cells and spacers. At scales 1.25 and 2, the Devanagari row still differs by 69 and 125 pixels respectively; the other multilingual differences are in box/block graphics. The decoration cases include faint, invisible, strike, overline, hyperlink, five underline styles, resolved inverse colors, selected foreground and low contrast. IME is checked over an existing selection.

Native appearance now isolates dictionary graphemes, retains their natural internal advances, aligns scalar clusters to their source cells, and uses the source sprite set. Heavy boxes and shades use actual font glyphs. Native scalar batches include spaces and wide-cell placeholders and share a baseline for their font group. On Linux that baseline uses the maximum ascent and descent of the actual glyph fonts, and placement follows the source conversion that omits layout glyph offsets. Source and candidate glyph IDs and fallback font metrics are recorded directly to distinguish shaping, placement and raster failures. These changes have been checked against the separate twelve-case baseline, which retains all 401,152 exact pixels and seven metrics.

Native curly waves use the analytic underline primitive. Its independent source comparison covers 64 solid/wavy cases and 122,880 exact pixels, plus a 4 MiB retained-record stress check. Source decoration placement and IME masking remain separate open corrections. The Metal constructor, upload and encode paths compile to LLVM in Linux semantic probes; actual Metal GPU execution remains untested here.

The native font-option diagnostics request synthesis when a styled request resolves to the same native face as its same-family request with that style removed. The Linux native rasterizer ignores synthetic bold/italic and font-smoothing hints; macOS consumes them. The font-case equality above therefore does not establish synthetic-style parity on macOS. The full corrected 24-case run peaked at 348 MiB with cached build artifacts; all builds and captures remain serialized under the guard.
