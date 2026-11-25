# Code editor

A virtualized, syntax-highlighted code editor built on a rope buffer with line numbers, soft wrap, multi-cursor selection, code folding, and optional Vim modal editing. Gate the crate on the `editor` feature (plus `tree-sitter` for 14 syntax grammars and fold extraction).

## Bordered editor and Vim mode

Click inside either editor below to move the caret, edit text, or try Vim normal/visual motions (`h j k l`, `w b`, `v`, `i`, `Esc`) in the second buffer.

```figure uses=code_editor
<script type="text/plain" id="docs-ce-rust">
use std::collections::HashMap;

pub struct Session {
    name: String,
    env: HashMap<&'static str, String>,
}

impl Session {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), env: HashMap::new() }
    }
}
</script>
<script type="text/plain" id="docs-ce-json">
{
  "tint": "blue",
  "radius": 12,
  "smoothing": 3,
  "glass": "liquid",
  "motion": "default"
}
</script>
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Rust · absolute lines</span><span class="docs-pr-col-meta">field.multiline</span></div>
    <div style="position: relative; width: 100%; height: 224px">
      <div class="penna-code-editor" data-penna="field.multiline" data-source="docs-ce-rust" data-language="rust" data-selection="0" aria-label="Rust code editor"></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">JSON · Vim + relative lines</span><span class="docs-pr-col-meta">data-vim</span></div>
    <div style="position: relative; width: 100%; height: 224px">
      <div class="penna-code-editor" data-penna="field.multiline" data-source="docs-ce-json" data-language="json" data-line-numbers="relative" data-vim data-selection="36" aria-label="JSON Vim code editor"></div>
    </div>
  </div>
</div>
```

```rust
let editor = cx.new(|cx| {
    CodeEditorState::new(window, cx)
        .language("rust")
        .default_value(source)
        .soft_wrap(true)
        .tab_size(TabSize { tab_size: 4, hard_tabs: false })
});

editor.update(cx, |state, cx| {
    state.set_relative_line_numbers(true, cx);
    state.set_vim_enabled(true, cx);
});

CodeEditor::new(&editor).bordered(true)
```

## Native vs HTML twin scope

- **Native (`editor` + `tree-sitter`):** 14 syntax grammars (`bash`, `c`, `css`, `diff`, `go`, `html`, `javascript`, `json`, `md`, `python`, `rust`, `toml`, `typescript`, `yaml`), tree-sitter code folding, `TabSize` config, placeholder text, and full Vim modes (`Normal`, `Insert`, `Visual`, `VisualLine`) with text objects.
- **HTML twin (`code_editor.js`):** lightweight interactive editor supporting `rust`, `json`, and plain `text`, `absolute` / `relative` / `off` line numbers, soft wrap, selection, and core Vim navigation without external dependencies.
- **Read-only prose or snippets:** use [Text](text.html) (`TextView`) for rendered markdown and copyable read-only code blocks; reserve `CodeEditor` for editable buffers.

## API

| Rust (`penna::blocks::code_editor`) | HTML (`uses=code_editor`) | Role |
| --- | --- | --- |
| `CodeEditorState::new(w, cx)` | `.penna-code-editor[data-source]` | Rope buffer, display map, selection |
| `CodeEditor::new(&state).bordered(true)` | `data-penna="field.multiline"` | Bordered multiline editor chrome |
| `.language("rust")` | `data-language="rust" \| "json" \| "text"` | Syntax grammar selection |
| `.set_line_numbers` / `.set_relative_line_numbers` | `data-line-numbers="absolute" \| "relative" \| "off"` | Gutter mode |
| `.soft_wrap(bool)` | `data-wrap` | Wrap lines to viewport width |
| `.set_vim_enabled(bool, cx)` | `data-vim` / `data-vim-visual="<offset>"` | Modal Vim keybindings |
| `.value()` / `.set_value(...)` / `.insert(...)` | DOM input / selection | Programmatic buffer reads and edits |
| `CodeEditorEvent::{Change, Focus, Blur}` | Native focus/input events | State notifications |
