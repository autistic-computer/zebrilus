# Palette

Finds and runs things from one field: places, commands and values, matched as you type. For a short fixed list use a [menu](menu.html). Press `⌘K` (or `Ctrl+K`) anywhere in these docs to try a live palette.

## Surface

The field, an optional usage line, the results, and a fixed four-slot footer of keys. Focus the field below and press `↑`, `↓`, `Ctrl+N`, or `Ctrl+P` to move the highlight.

```figure uses=palette,badge,kbd,list,panel
<div class="docs-show" data-gap="wide" style="width: 100%; flex-direction: column; align-items: stretch">
  <div class="docs-cell" style="width: 100%">
    <div class="penna-panel penna-palette-surface" data-penna="popover" data-ground role="region" aria-label="Command palette" data-inset="8">
      <label class="penna-palette-input">
        <span class="penna-palette-pills">
          <span class="penna-badge penna-palette-pill" data-penna="badge" data-kind="status"><span class="penna-palette-dot" data-status="online"></span><span data-penna-text>gpu-box</span></span>
          <span class="penna-badge penna-palette-pill" data-penna="badge" data-kind="mode"><span class="penna-palette-prefix" data-penna-text>:</span><span data-penna-text>Command</span></span>
          <span class="penna-badge penna-palette-pill" data-penna="badge" data-kind="code"><span data-penna-text>join-pane</span></span>
        </span>
        <input value="" placeholder="Target window" aria-label="Target window" />
      </label>
      <div class="penna-palette-usage" data-penna-text>join-pane [-bdfhv] [-l size] [-s src-pane] [-t dst-pane]</div>
      <div class="penna-palette-rows" role="listbox" aria-label="Windows">
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="panels-top-left"></i><span class="penna-palette-label" data-penna-text>editor</span><span class="penna-palette-detail" data-penna-text>2 panes</span><span class="penna-palette-right" data-penna-text>current</span><span class="penna-palette-inside"><i data-icon="chevron-right"></i></span></div></div>
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="panels-top-left"></i><span class="penna-palette-label" data-penna-text>agents</span><span class="penna-palette-detail" data-penna-text>2 panes</span><span class="penna-palette-dot" data-status="busy"></span><span class="penna-palette-inside"><i data-icon="chevron-right"></i></span></div></div>
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="panels-top-left"></i><span class="penna-palette-label" data-penna-text>scratch</span><span class="penna-palette-detail" data-penna-text>1 pane</span><span class="penna-palette-inside"></span></div></div>
      </div>
      <div class="penna-palette-footer">
        <div class="penna-palette-rule" data-penna="rule.bar"></div>
        <span class="penna-palette-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="enter"></span></span><span class="penna-palette-words"><span data-on data-penna-text>run</span><span data-penna-text>switch</span></span></span>
        <span class="penna-palette-hint" data-state="disabled"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="alt-enter"></span></span><span data-penna-text>take</span></span>
        <span class="penna-palette-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="tab"></span></span><span class="penna-palette-words"><span data-on data-penna-text>inside</span><span data-penna-text>complete</span></span></span>
        <span class="penna-palette-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="shift-tab"></span></span><span data-penna-text>back</span></span>
      </div>
    </div>
    <span class="docs-note">Status, mode and code pills, usage line, drill-in chevron slot, and fixed 4-slot PaletteKeys footer</span>
  </div>
  <div class="docs-cell" style="width: 100%">
    <div class="penna-panel penna-palette-surface" data-penna="popover" data-ground data-still data-input-last role="region" aria-label="Bottom-input prompt palette" data-inset="8">
      <div class="penna-palette-rows" role="listbox" aria-label="Recent commands">
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><span class="penna-palette-label" data-penna-text>select-window -t 2</span><span class="penna-badge penna-palette-kind" data-penna="badge" data-size="small"><span class="badge-text" data-penna-text>HISTORY</span></span></div></div>
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-detail><div class="penna-palette-content"><span class="penna-palette-label" data-penna-text>new-window -n logs</span><span class="penna-badge penna-palette-kind" data-penna="badge" data-size="small"><span class="badge-text" data-penna-text>HISTORY</span></span></div></div>
      </div>
      <label class="penna-palette-input" data-prompt data-mono>
        <span class="penna-palette-prompt" data-penna-text>:</span>
        <input value="new-window -n logs" placeholder="Type a command" aria-label="Command prompt" readonly />
      </label>
      <div class="penna-palette-footer">
        <div class="penna-palette-rule" data-penna="rule.bar"></div>
        <div class="penna-palette-footer-leading"><span class="penna-badge penna-palette-vim" data-penna="badge" data-tone="primary" data-emphasis="solid"><span class="badge-text" data-penna-text>NORMAL</span></span></div>
        <span class="penna-palette-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="enter"></span></span><span data-penna-text>run</span></span>
        <span class="penna-palette-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="tab"></span></span><span data-penna-text>complete</span></span>
      </div>
    </div>
    <span class="docs-note">Bottom input (<code>.input_last()</code>), mono prompt, and leading vim badge (<code>.footer_leading()</code>)</span>
  </div>
</div>
```

```rust
PaletteSurface::new(
    palette_input(
        &input,
        [PalettePill::Mode { prefix: ":".into(), label: "Command".into() }],
        cx,
    ),
    revision,
)
.usage("join-pane [-bdfhv] [-s src-pane] [-t dst-pane]")
.rows(rows)
.keys(
    PaletteKeys::new(&["run", "switch"], "run", &["inside", "complete"])
        .inside(true)
        .back(true),
)
```

## Rows

One result per row: an icon or status dot, label with matched letters or a muted path prefix, detail, right-side note or shortcut, and an optional drill-in chevron. Tree rows add a disclosure toggle (click the chevrons below).

```figure uses=palette,badge,kbd,list,panel
<div class="penna-panel penna-palette-surface" data-penna="popover" data-ground role="region" aria-label="Palette row kinds" data-inset="8">
  <div class="penna-palette-rows" role="listbox" aria-label="Row kinds">
    <div class="penna-palette-section"><span data-penna-text>Commands</span><span class="penna-palette-section-hint" data-penna-text>:</span></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="terminal"></i><span class="penna-palette-label" data-penna-text><span class="penna-palette-match">spl</span>i<span class="penna-palette-match">t</span>-window</span><span class="penna-palette-detail" data-penna-text>Split a pane into two</span><span class="penna-palette-shortcut"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="ctrl-b" data-kbd-lowercase></kbd><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="%" data-kbd-lowercase></kbd></span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="square-terminal"></i><span class="penna-palette-label" data-penna-text><span class="penna-palette-muted">work / editor / </span>cargo watch</span><span class="penna-palette-detail" data-penna-text>Terminal</span><span class="penna-palette-shortcut"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-mono><span data-penna-text>prefix %</span></kbd></span></div></div>
    <div class="penna-palette-section"><span data-penna-text>Hosts</span><span class="penna-palette-section-hint" data-penna-text>@</span></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="hard-drive"></i><span class="penna-palette-dot" data-status="online"></span><span class="penna-palette-label" data-penna-text>gpu-box</span><span class="penna-palette-detail" data-penna-text>ssh gpu-box</span><span class="penna-palette-right" data-penna-text>1 session</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="hard-drive"></i><span class="penna-palette-dot" data-status="running"></span><span class="penna-palette-label" data-penna-text>ci</span><span class="penna-palette-detail" data-penna-text>ssh ci</span><span class="penna-palette-right" data-penna-text>building</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="hard-drive"></i><span class="penna-palette-dot" data-status="waiting"></span><span class="penna-palette-label" data-penna-text>staging</span><span class="penna-palette-detail" data-penna-text>ssh deploy@staging</span><span class="penna-palette-right" data-penna-text>connecting</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="hard-drive"></i><span class="penna-palette-dot" data-status="offline"></span><span class="penna-palette-label" data-penna-text>pi</span><span class="penna-palette-detail" data-penna-text>ssh pi.local</span><span class="penna-palette-right" data-penna-text>offline</span></div></div>
    <div class="penna-palette-section"><span data-penna-text>Workspace tree</span></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail aria-expanded="true"><div class="penna-palette-content"><span class="penna-palette-disclosure" data-toggle><i data-icon="chevron-down"></i></span><i class="penna-palette-icon" data-icon="folder"></i><span class="penna-palette-label" data-penna-text>work</span><span class="penna-palette-detail" data-penna-text>2 windows</span><span class="penna-palette-right" data-penna-text>current</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail aria-expanded="true" style="--indent: 18px"><div class="penna-palette-content"><span class="penna-palette-disclosure" data-toggle><i data-icon="chevron-down"></i></span><i class="penna-palette-icon" data-icon="panels-top-left"></i><span class="penna-palette-label" data-penna-text>editor</span><span class="penna-palette-detail" data-penna-text>2 panes</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail style="--indent: 36px"><div class="penna-palette-content"><span class="penna-palette-disclosure"></span><i class="penna-palette-icon" data-icon="square-terminal"></i><span class="penna-palette-label" data-penna-text>nvim</span><span class="penna-palette-detail" data-penna-text>Terminal</span><span class="penna-palette-right" data-penna-text>current</span></div></div>
    <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail aria-expanded="false" style="--indent: 18px"><div class="penna-palette-content"><span class="penna-palette-disclosure" data-toggle><i data-icon="chevron-right"></i></span><i class="penna-palette-icon" data-icon="panels-top-left"></i><span class="penna-palette-label" data-penna-text>agents</span><span class="penna-palette-detail" data-penna-text>2 panes</span><span class="penna-palette-dot" data-status="busy"></span></div></div>
  </div>
</div>
```

```rust
palette_row(
    "new-window",
    &PaletteRow {
        label: "new-window".into(),
        detail: "Create a new window".into(),
        icon: Some(IconName::Terminal),
        shortcut: Some(PaletteShortcut::Keys(keys)),
        inside: Some(false),
        ..Default::default()
    },
    selected,
    cx,
)
```

## Parts and keyboard

Section headers, kind badges, empty states, inline key hints, and vim mode badges compose the rest of a picker.

```figure uses=palette,badge,kbd,list,panel
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Kind badges &amp; empty</span>
    </div>
    <div class="penna-panel penna-palette-surface" data-penna="popover" data-ground data-still role="region" aria-label="Kind badges" data-inset="8">
      <div class="penna-palette-rows" role="listbox" aria-label="Completions">
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-detail><div class="penna-palette-content"><span class="penna-palette-label" data-penna-text>-t</span><span class="penna-palette-detail" data-penna-text>Target window</span><span class="penna-badge penna-palette-kind" data-penna="badge" data-size="small"><span class="badge-text" data-penna-text>OPTION</span></span></div></div>
        <div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" aria-selected="false" data-detail><div class="penna-palette-content"><span class="penna-palette-label" data-penna-text>work:1</span><span class="penna-palette-detail" data-penna-text>editor</span><span class="penna-badge penna-palette-kind" data-penna="badge" data-size="small"><span class="badge-text" data-penna-text>VALUE</span></span></div></div>
      </div>
      <div class="penna-palette-empty" style="border-top: 1px solid var(--penna-divider)" data-penna-text>No matches</div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Key hints &amp; vim modes</span>
    </div>
    <div class="docs-show" data-gap="wide" style="flex-direction: column; align-items: flex-start; gap: 16px; padding-top: 4px">
      <div class="docs-show" style="gap: 16px">
        <span class="penna-palette-key-hint"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd><span data-penna-text>palette</span></span>
        <span class="penna-palette-key-hint"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="w"></kbd><span data-penna-text>choose window</span></span>
      </div>
      <div class="docs-show">
        <span class="penna-badge penna-palette-vim" data-penna="badge"><span class="badge-text" data-penna-text>INSERT</span></span>
        <span class="penna-badge penna-palette-vim" data-penna="badge" data-tone="primary" data-emphasis="solid"><span class="badge-text" data-penna-text>NORMAL</span></span>
        <span class="penna-badge penna-palette-vim" data-penna="badge" data-tone="primary" data-emphasis="solid"><span class="badge-text" data-penna-text>NORMAL g</span></span>
      </div>
    </div>
  </div>
</div>
```

- **Navigation**: `Up`, `Down`, `Ctrl+N`, and `Ctrl+P` (`palette_chord`) move selection and wrap; `Enter` runs the picked row; `Alt+Enter` runs the secondary action (`take` or `copy`); `Tab` drills in or completes; `Shift+Tab` steps back.
- **Footer**: pickers use the fixed four-slot `PaletteKeys` strip (`enter`, `alt-enter`, `tab`, `shift-tab`) so key positions never jump while typing — no arrow or Escape hints.
- **Vim**: `PaletteVim` starts in insert mode; `Escape` drops to normal mode (`j`/`k`, `gg`/`G`, `/`, `i`, `a`), and a second `Escape` closes the palette.

```rust
palette_section("Commands", ":", cx)
palette_empty("No matches", cx)
palette_badged_row("-t", &row, false, palette_kind_badge("OPTION", mono), cx)
palette_vim_badge(PaletteVimMode::Normal, Some('g'), cx)
```

## API (`penna::blocks::palette`)

| Rust | HTML | Does |
| --- | --- | --- |
| `PaletteSurface::new(input, revision)` | `.penna-palette-surface[data-inset="8"]` | Floating palette surface (`max_width` 560 px) |
| `.max_width(px)`, `.input_last()`, `.over_input(el)` | `[data-input-last]` | Width override, bottom input order, overlay above input |
| `.usage(text)`, `.reserve_usage()` | `.penna-palette-usage` | Mono usage line or fixed placeholder room |
| `.rows(el)`, `.hints(hints)`, `.keys(PaletteKeys)` | `.penna-palette-rows`, `.penna-palette-footer` | Result list and footer key strip |
| `.actions(actions)` / `PaletteAction::new(label)` | `.penna-palette-footer` | Clickable footer actions (`.key(...)`, `.destructive()`, `.on_click(...)`) |
| `.footer_leading(el)` | `.penna-palette-footer-leading` | Leading footer slot (e.g. vim badge) |
| `PaletteKeys::new(verbs, verb, tabs)` | `.penna-palette-hint`, `.penna-palette-words` | Fixed 4-slot key strip (`.take`, `.copy`, `.inside`, `.complete`, `.back`) |
| `palette_input(&state, pills, cx)` | `.penna-palette-input`, `.penna-palette-pills` | Input field led by `PalettePill` badges |
| `palette_prompt_input(&state, prompt, font, cx)` | `.penna-palette-input[data-prompt][data-mono]` | Input field led by a mono prompt |
| `PalettePill::{Status, Mode, Code, Crumb, Action}` | `.penna-palette-pill[data-kind]` | Context pills ahead of the input |
| `palette_row(id, &row, selected, cx)` | `.penna-palette-row[data-penna="menu-item"]` | Result row (`matches`, `muted_prefix`, `right`, `shortcut`) |
| `palette_tree_row(id, &row, selected, on_toggle, cx)` | `.penna-palette-disclosure[data-toggle]` | Indented row with disclosure toggle (`indent`, `expanded`) |
| `palette_badged_row(id, &row, selected, badge, cx)` | `.penna-palette-kind` | Row with a trailing `palette_kind_badge` |
| `PaletteRow { inside: Some(true), .. }` | `.penna-palette-inside` | Trailing chevron (`Some(false)` reserves slot, `None` omits) |
| `PaletteRow { status, running, .. }` | `.penna-palette-dot[data-status]` | `PaletteStatus` (`Online`, `Running`, `Waiting`, `Offline`) or busy dot |
| `PaletteShortcut::{Keys, Text}`, `palette_shortcut` | `.penna-palette-shortcut` | Keycaps or mono text shortcut |
| `palette_section`, `palette_empty`, `palette_key_hint` | `.penna-palette-section`, `.penna-palette-empty`, `.penna-palette-key-hint` | Section headers, empty state, and inline key hints |
| `palette_chord`, `palette_list_height`, `palette_row_height` | `palette.js` | `Ctrl-N`/`Ctrl-P` chord helper and height calculations |
| `PaletteVim`, `PaletteVimMode`, `palette_vim_badge` | `.penna-palette-vim` | Insert/normal vim state machine and badge |
