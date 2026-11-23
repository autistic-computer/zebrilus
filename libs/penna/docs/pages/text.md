# Text

Markdown for reading: documents, help and chat. Larger and more open than interface text; for a single UI line use a [label](label.html).

## Kinds and reading ladder

Three forms: **Fixed** (`TextView::markdown`) for static prose, **Stateful** (`TextView::new(&state)`) for live or streaming text via `TextViewState`, and **Inherited** (`TextViewStyle::text(None)`) which adopts the container's text style and drops the 640 px measure.

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Tag</span><span class="docs-head">Sample</span>
  <span><code>read-h1</code></span><span><code>26 / 32</code></span><span>h1</span><span class="penna-type" data-type="read-h1" data-penna-text>Sessions</span>
  <span><code>read-h2</code></span><span><code>20 / 28</code></span><span>h2</span><span class="penna-type" data-type="read-h2" data-penna-text>Windows</span>
  <span><code>read-h3</code></span><span><code>16 / 24</code></span><span>h3</span><span class="penna-type" data-type="read-h3" data-penna-text>Panes</span>
  <span><code>read-h4</code></span><span><code>14 / 22</code></span><span>h4</span><span class="penna-type" data-type="read-h4" data-penna-text>Keys</span>
  <span><code>read-h5</code></span><span><code>13 / 20</code></span><span>h5, h6</span><span class="penna-type" data-type="read-h5" data-penna-text>Copy mode</span>
  <span><code>read-body</code></span><span><code>14 / 22</code></span><span>p</span><span class="penna-type" data-type="read-body" data-penna-text>The server owns every process.</span>
</div>
```

Headings are 600, body 400, bold 600. Lines stop at `READING_MEASURE` 640.

```rust
TextView::markdown("notes", source).selectable(true)
```

## Rhythm

Blocks sit 12 apart. Headings have 24 above at h1 and h2, 16 above at h3 to h6, and 8 below.

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-md" style="width: min(100%, 280px)"><h2>Windows</h2><p>A window holds panes side by side.</p><p>Close the last pane to close it.</p></div><span class="docs-note">h2, then two paragraphs</span></div>
  <div class="docs-cell"><div class="penna-md" style="width: min(100%, 280px)"><p>The server keeps every pane alive.</p><h3>Panes</h3><p>Each pane runs one process.</p></div><span class="docs-note">h3 after a paragraph</span></div>
</div>
```

## Lists, tasks and selection

Task lists, strikethrough, autolinks, math code spans, callouts and footnotes are enabled by default on `MarkdownExtensions::default()`. Selectable text paints a 28% foreground wash across drag, double-click, triple-click and Cmd+A / Cmd+C.

```figure
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: min(100%, 280px)">
    <div class="penna-md">
      <ul>
        <li><div class="md-item"><span class="md-box"><span class="md-check" role="checkbox" aria-readonly="true" aria-checked="true"><i data-icon="check"></i></span></span><div class="md-body"><p>Run token parity</p></div></div></li>
        <li><div class="md-item"><span class="md-box"><span class="md-check" role="checkbox" aria-readonly="true" aria-checked="false"></span></span><div class="md-body"><p>Update docs figures</p></div></div></li>
      </ul>
    </div>
    <span class="docs-note">Task list</span>
  </div>
  <div class="docs-cell" style="width: min(100%, 280px)">
    <div class="penna-md" data-selectable data-selected><p>The server <span class="md-sel">owns every pane</span> across reconnects.</p></div>
    <span class="docs-note">Selection wash (28%)</span>
  </div>
</div>
```

## Code

Code takes the reading size in the code face. Blocks sit on a `card.code` card. With the `tree-sitter` feature, fences in Rust, Python, JavaScript, TypeScript, TSX, shell, Go, C, CSS, HTML, JSON, TOML, YAML, Markdown and diff are colored (natively, diff added and removed lines take success and danger inks).

```figure
<div class="penna-md docs-full"><p>Run <code data-penna="card.code">cargo run</code> to start the server.</p><div class="md-code"><pre data-penna="card.code"><code><span class="tk-keyword">let</span> pane = session.<span class="tk-function">active_pane</span>();</code></pre></div></div>
```

## Quotes, callouts and tables

A quote has a 4 px bar and 16 px inside it. Natively, GitHub-style alerts (`[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`) and `:::name title` block directives draw as a `card.activity` card washed in their tone with a toned icon and semibold title in default ink. Table columns size to their content from 48 to 480 and share the width; `data-scroll` keeps those widths and scrolls sideways.

```figure
<div class="penna-md" style="width: min(100%, 340px)"><div class="md-quote"><blockquote><p>The server outlives the app.</p></blockquote></div><div class="md-table"><div class="md-figure" data-penna="card.figure"><div class="md-row"><div class="md-cell" data-align="left">Command</div><div class="md-cell" data-align="center">Key</div></div><div class="md-row"><div class="md-cell" data-align="left">Split right</div><div class="md-cell" data-align="center"><code data-penna="card.code">Ctrl+D</code></div></div></div></div></div>
```

## Footnotes and streaming

A footnote reference is a superscript number; notes gather after the last block in caption size. Streaming text fades new chunks in over 120 to 220ms (instant under reduced motion).

```figure
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: min(100%, 280px)"><div class="penna-md"><p>The server outlives the app.¹</p><div class="md-footnotes"><div class="md-footnote"><span>1.</span><p>Until the machine restarts.</p></div></div></div><span class="docs-note">Footnotes</span></div>
  <div class="docs-cell" style="width: min(100%, 280px)"><div class="penna-md" data-streaming><p>Splitting the pane now.</p><template data-tail><p>The new pane keeps the working directory.</p></template></div><span class="docs-note">Streaming fade</span></div>
</div>
```

## Inherited and structure

`TextViewStyle::text(None)` takes the container's text style and drops the 640 px measure while headings keep the reading ladder. Use headings for structure, never for emphasis.

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-md" data-text="inherit" style="width: min(100%, 280px)"><h4>Split the pane</h4><p>The new pane keeps the working directory, so the build stays warm.</p></div><span class="docs-note">In a body container, 13 on 18</span></div>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `TextView::markdown(id, text)` | `.penna-md` | Static markdown view |
| `TextView::new(&state)` | `.penna-md` | Stateful view backed by `TextViewState` |
| `TextViewState::markdown(..)`, `.push_str(..)`, `.set_text(..)`, `.selected_text()` | | Creates, appends, replaces, or reads selected text |
| `.selectable(true)` | `data-selectable`, `data-selected` | Select and copy |
| `.streaming(true)` | `data-streaming`, `template[data-tail]` | Text still arriving |
| `.scrollable(true)` | none | Fills its container and scrolls |
| `.code_block_actions(f)`, `.code_block_body(f)` | `data-actions`, `.md-actions` | Buttons or custom renderers on fenced blocks |
| `.markdown_extensions(ext)` | none | Custom block plugins/parsers, `link_rewriter`, `inline_links` |
| `.links(impl TextLinks)` | `a` | Custom link styling, tooltips, and click handling |
| `.style(TextViewStyle::default().text(None))` | `data-text="inherit"` | Inherits container style, drops `READING_MEASURE` |
| `TextViewStyle::table(style)`, `.paragraph_gap(gap)` | `data-scroll`, `.md-scroll` | Horizontal-scrolling tables and block spacing |
