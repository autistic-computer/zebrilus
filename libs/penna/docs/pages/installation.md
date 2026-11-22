# Installation

penna builds for native `ala` windows in Rust and for the web in plain HTML and CSS. Pick a target below to configure the minimal setup.

## Setup

```figure uses=notice,settings,menu,chip
<div class="docs-install" data-docs-install>
  <div class="docs-install-bar">
    <div class="penna-segmented" data-penna="field" data-value="0" data-install-target role="radiogroup" aria-label="Target">
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Rust (ala)</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>HTML / CSS</span></span>
      </div>
    </div>
    <span class="docs-install-meta" data-install-meta>Rust 1.99 · Edition 2024</span>
  </div>

  <div class="docs-install-panel" data-install-panel="rust">
    <div class="docs-install-flow">
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">01</span>
          <i class="docs-install-stage-icon" data-icon="palette" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">penna::Assets</div>
        <p class="docs-install-stage-copy">Registers built-in icons on the application.</p>
      </div>
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">02</span>
          <i class="docs-install-stage-icon" data-icon="sliders" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">penna::init(cx)</div>
        <p class="docs-install-stage-copy">Loads tokens, keymaps and the platform look.</p>
      </div>
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">03</span>
          <i class="docs-install-stage-icon" data-icon="app-window" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">Root::new</div>
        <p class="docs-install-stage-copy">Hosts popovers, dialogs and base 13/18 type.</p>
      </div>
    </div>

    <div class="docs-install-card" data-penna="settings.card" data-ground>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>tree-sitter</code></span>
          <span class="docs-install-row-desc">Syntax highlighting for markdown and code blocks</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="tree-sitter feature" data-install-feature="tree-sitter"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>editor</code></span>
          <span class="docs-install-row-desc">Multi-line code editor block with gutters and selection</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="editor feature" data-install-feature="editor"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>tray</code></span>
          <span class="docs-install-row-desc">Native menu bar and system tray host</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="tray feature" data-install-feature="tray"></button>
      </div>
    </div>

    <div class="penna-md">
      <div class="md-code">
        <pre data-penna="card.code" data-ground data-actions><code data-install-cargo></code><div class="md-actions"><span class="md-lang" data-penna-text>Cargo.toml</span><div><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Copy" data-tooltip="Copy" data-copy><i data-icon="copy"></i></button></div></div></pre>
      </div>
      <div class="md-code">
        <pre data-penna="card.code" data-ground data-actions><code><span class="tk-keyword">fn</span> <span class="tk-function">main</span>() {
    ala_platform::<span class="tk-function">application</span>()
        .<span class="tk-function">with_assets</span>(penna::<span class="tk-type">Assets</span>)
        .<span class="tk-function">run</span>(|cx| {
            penna::<span class="tk-function">init</span>(cx);
            cx.<span class="tk-function">open_window</span>(<span class="tk-type">WindowOptions</span>::<span class="tk-function">default</span>(), |window, cx| {
                <span class="tk-keyword">let</span> view = cx.<span class="tk-function">new</span>(|_| <span class="tk-type">AppView</span>);
                cx.<span class="tk-function">new</span>(|cx| penna::<span class="tk-type">Root</span>::<span class="tk-function">new</span>(view, window, cx))
            }).<span class="tk-function">ok</span>();
        });
}</code><div class="md-actions"><span class="md-lang" data-penna-text>src/main.rs</span><div><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Copy" data-tooltip="Copy" data-copy><i data-icon="copy"></i></button></div></div></pre>
      </div>
    </div>
  </div>

  <div class="docs-install-panel" data-install-panel="html" hidden>
    <div class="docs-install-flow">
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">01</span>
          <i class="docs-install-stage-icon" data-icon="terminal" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">theme_css</div>
        <p class="docs-install-stage-copy">Generates light, dark and icon stylesheets into web/tokens.</p>
      </div>
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">02</span>
          <i class="docs-install-stage-icon" data-icon="layers" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">penna.css + kit.js</div>
        <p class="docs-install-stage-copy">Wires surfaces, superellipse corners, focus rings and glass.</p>
      </div>
      <div class="docs-install-stage" data-penna="card" data-ground>
        <div class="docs-install-stage-top">
          <span class="docs-install-stage-num">03</span>
          <i class="docs-install-stage-icon" data-icon="file" aria-hidden="true"></i>
        </div>
        <div class="docs-install-stage-name">Widget files</div>
        <p class="docs-install-stage-copy">Import only the CSS and JS for components you render.</p>
      </div>
    </div>

    <div class="docs-install-card" data-penna="settings.card" data-ground>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>button</code></span>
          <span class="docs-install-row-desc">Buttons, icon buttons and loading states</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Include button" data-install-widget="button"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>input</code></span>
          <span class="docs-install-row-desc">Single and multi-line text fields</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="Include input" data-install-widget="input"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>popover</code></span>
          <span class="docs-install-row-desc">Floating panels and liquid glass surfaces</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="Include popover" data-install-widget="popover"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title"><code>switch</code></span>
          <span class="docs-install-row-desc">Boolean toggle switches</span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="false" aria-label="Include switch" data-install-widget="switch"></button>
      </div>
      <div class="docs-install-row">
        <div class="docs-install-row-copy">
          <span class="docs-install-row-title">Hide built-in debug switcher</span>
          <span class="docs-install-row-desc">Adds <code>data-switcher="off"</code> to <code>kit.js</code></span>
        </div>
        <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Hide debug switcher" data-install-switcher></button>
      </div>
    </div>

    <div class="penna-md">
      <div class="md-code">
        <pre data-penna="card.code" data-ground data-actions><code>cargo run --example theme_css</code><div class="md-actions"><span class="md-lang" data-penna-text>bash</span><div><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Copy" data-tooltip="Copy" data-copy><i data-icon="copy"></i></button></div></div></pre>
      </div>
      <div class="md-code">
        <pre data-penna="card.code" data-ground data-actions><code data-install-html></code><div class="md-actions"><span class="md-lang" data-penna-text>index.html</span><div><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Copy" data-tooltip="Copy" data-copy><i data-icon="copy"></i></button></div></div></pre>
      </div>
    </div>
  </div>
</div>
```

## What a booted window gives you

Wrapping your view in `Root` with `penna::Assets` and `penna::init` wires tokens, icons and overlays before you build any screen. Click through the miniature window below to try each layer live.

```figure uses=notice
<div class="penna-window docs-install-win" data-install-win>
  <div class="penna-lights" aria-hidden="true"><i></i><i></i><i></i></div>
  <aside data-penna="sidebar" data-ground>
    <div class="penna-strip"></div>
    <div data-penna="sidebar.row" data-win-tab="tokens" data-win-label="Tokens" aria-current="page" data-state="selected" role="button" tabindex="0">
      <span class="penna-marker"><i data-icon="palette"></i></span>
      <span class="penna-label" data-penna-text>Tokens</span>
    </div>
    <div data-penna="sidebar.row" data-win-tab="overlays" data-win-label="Overlays" role="button" tabindex="0">
      <span class="penna-marker"><i data-icon="layers"></i></span>
      <span class="penna-label" data-penna-text>Overlays</span>
    </div>
    <div data-penna="sidebar.row" data-win-tab="icons" data-win-label="Icons" role="button" tabindex="0">
      <span class="penna-marker"><i data-icon="sparkle"></i></span>
      <span class="penna-label" data-penna-text>Icons</span>
    </div>
  </aside>
  <header data-penna="titlebar" data-ground>
    <span class="docs-install-win-title" data-win-title>Tokens</span>
    <span class="docs-install-win-meta" data-win-mode-readout>Dark · Liquid glass</span>
  </header>
  <div data-penna="pane" data-ground>
    <div class="docs-win-view" data-win-view="tokens">
      <p class="docs-win-copy">Every surface derives its fill, hairline and ink from one set of mode tokens.</p>
      <div class="docs-win-swatches">
        <div class="docs-win-swatch"><span class="docs-win-swatch-chip" style="background: var(--penna-bg)"></span><span class="docs-win-swatch-label">bg</span></div>
        <div class="docs-win-swatch"><span class="docs-win-swatch-chip" style="background: var(--penna-panel)"></span><span class="docs-win-swatch-label">panel</span></div>
        <div class="docs-win-swatch"><span class="docs-win-swatch-chip" style="background: var(--penna-accent)"></span><span class="docs-win-swatch-label">accent</span></div>
        <div class="docs-win-swatch"><span class="docs-win-swatch-chip" style="background: var(--penna-fg)"></span><span class="docs-win-swatch-label">fg</span></div>
      </div>
      <div class="docs-win-actions">
        <button class="penna-button" data-penna="button" data-size="small" data-win-toggle-mode><i data-icon="sun" data-docs-mode-icon="dark"></i><i data-icon="moon" data-docs-mode-icon="light"></i><span class="penna-label" data-penna-text>Flip mode</span></button>
      </div>
    </div>

    <div class="docs-win-view" data-win-view="overlays" hidden>
      <p class="docs-win-copy">Root hosts popovers, dialogs, tooltips and toasts above your panes so clipping and dismissal work out of the box.</p>
      <div>
        <div class="penna-popover docs-win-pop-wrap" data-win-popover-host>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" aria-expanded="false" data-win-popover-btn><span class="penna-label" data-penna-text>Open popover</span></button>
          <div class="penna-popover-panel" data-penna="popover" data-ground data-win-popover>
            <div class="docs-win-pop-title">Hosted by Root</div>
            <div class="docs-win-pop-text">Follows the header Glass knob and dismisses on Escape or outside click.</div>
          </div>
        </div>
      </div>
    </div>

    <div class="docs-win-view" data-win-view="icons" hidden>
      <p class="docs-win-copy">penna::Assets embeds the full icon set and adapts stroke corners to your radius knob.</p>
      <div class="docs-win-icons">
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-state="current" data-win-icon="Terminal" aria-label="Terminal"><i data-icon="terminal"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="PanelLeft" aria-label="PanelLeft"><i data-icon="panel-left"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Search" aria-label="Search"><i data-icon="search"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Sliders" aria-label="Sliders"><i data-icon="sliders"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Palette" aria-label="Palette"><i data-icon="palette"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Layers" aria-label="Layers"><i data-icon="layers"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Check" aria-label="Check"><i data-icon="check"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-win-icon="Copy" aria-label="Copy"><i data-icon="copy"></i></button>
      </div>
      <div class="docs-win-icon-readout" data-win-icon-name>IconName::Terminal</div>
    </div>
  </div>
</div>
```

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="neutral">
  <div class="penna-notice-tone" data-penna="chip">
    <span class="penna-notice-lead"><i data-icon="info"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Tokens stay in sync from Rust</span>
      <span class="penna-notice-detail">HTML builds read CSS generated from the Rust token engine. Re-run theme_css after updating penna so web surfaces match native windows.</span>
    </div>
  </div>
</div>
```
