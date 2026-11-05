# Palette

Finds and runs things from one field: places, commands and values, matched as you type. For a short fixed list use a menu.

## Surface

The field, a usage line, the results and a footer of keys.

```rust example story=palette/surface
PaletteSurface::new(
    palette_input(&input, [PalettePill::Mode { prefix: ":".into(), label: "Command".into() }], cx),
    revision,
)
.usage("join-pane [-bdfhv] [-s src-pane] [-t dst-pane]")
.rows(rows)
.hints([PaletteHint { key: "enter", label: "run" }])
```

## Rows

One result per row: an icon or a status dot, the label with its matched letters, a detail, and its keys. Tree rows add a disclosure.

```rust example story=palette/rows
palette_row("new-window", &PaletteRow {
    label: "new-window".into(),
    detail: "Create a new window".into(),
    icon: Some(IconName::Terminal),
    shortcut: Some(PaletteShortcut::Keys(keys)),
    ..Default::default()
}, selected, cx)
```

## Parts

Section headers, the empty result, kind badges, key hints and the vim badge.

```rust example story=palette/parts
palette_section("Commands", ":", cx)
palette_empty("No matches", cx)
palette_badged_row("-t", &row, false, palette_kind_badge("OPTION", mono), cx)
palette_vim_badge(PaletteVimMode::Normal, Some('g'), cx)
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `PaletteSurface::new(input, revision)` | `.penna-palette-surface` with `data-inset="8"` | The palette |
| `palette_input(&state, pills, cx)` | `.penna-palette-input` with `.penna-palette-pills` | Field led by pills |
| `palette_prompt_input(&state, prompt, font, cx)` | `.penna-palette-input[data-prompt]` | Field led by a prompt |
| `PalettePill::Status`, `Mode`, `Code`, `Crumb`, `Action` | `.penna-palette-pill[data-kind]` | Pills |
| `palette_row(id, &row, selected, cx)` | `.penna-palette-row` with `data-penna="menu-item"` | Row |
| `palette_tree_row(id, &row, selected, on_toggle, cx)` | `.penna-palette-disclosure[data-toggle]` | Row with a disclosure |
| `palette_badged_row(id, &row, selected, badge, cx)` | a badge in the row | Row with a kind badge |
| `PaletteRow { matches, muted_prefix, .. }` | `.penna-palette-match`, `.penna-palette-muted` | Matched letters, muted path |
| `PaletteRow { status, running, .. }` | `.penna-palette-dot[data-status]` | Status and running dots |
| `PaletteRow { shortcut: Some(PaletteShortcut::Keys(..)), .. }` | `.penna-palette-shortcut` | Keys |
| `.usage(text)`, `.reserve_usage()` | `.penna-palette-usage` | Usage line |
| `.hints(hints)` | `.penna-palette-hint` | Footer keys |
| `palette_section(label, hint, cx)` | `.penna-palette-section` | Section header |
| `palette_empty(label, cx)` | `.penna-palette-empty` | Nothing matched |
| `PaletteVim`, `palette_vim_badge(mode, pending, cx)` | `.penna-palette-vim` | Vim modes |
