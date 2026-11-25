# Showcase

Real screens composed from penna's components and blocks — the terminal multiplexer (`zz`) across desktop and phone, the multi-machine desk manager (`nidus`), and live GPU-to-HTML parity measured from every storybook baseline.

```figure uses=badge,card,chat,chip,compact,dock,icon,input,inspector,kbd,label,list,notice,panel,progress,row,segmented,settings,sidebar,spinner,switch,tab,tile,title_bar,touch
<div class="docs-sc-toolbar" data-penna="card" data-ground data-docs-showcase>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Mode</span>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Mode" data-sc-mode>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Dark</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Light</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Density</span>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" role="radiogroup" aria-label="Density" data-sc-density>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Compact</span></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Regular</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Comfort</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-ctrl">
    <span class="docs-sc-ctrl-label">Glass</span>
    <button class="penna-toggle" role="switch" data-size="small" aria-checked="true" aria-label="Liquid glass" type="button" data-sc-glass></button>
  </div>
</div>
```

## zz workspace

Switch windows from the flat sidebar or titlebar tabs, expand multi-pane rows, toggle the sidebar or split pane, send a message in the composer, or flip to the 390 px phone view.

```figure uses=badge,card,chat,chip,compact,icon,input,label,list,panel,row,segmented,sidebar,spinner,tab,tile,title_bar,touch
<div class="docs-sc-section" data-sc-zz>
  <div class="docs-sc-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="zz viewport" data-sc-zz-view>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Desktop</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Phone</span></span>
      </div>
    </div>
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button" data-sc-zz-split aria-pressed="true"><i data-icon="panel-right"></i><span class="penna-label" data-penna-text>Split pane</span></button>
  </div>

  <div class="docs-sc-zz-desktop" data-sc-zz-pane="desktop">
    <div class="penna-window penna-gaps docs-zz-win" data-penna="window" data-ground data-sc-zz-win>
      <div class="penna-lights" aria-hidden="true"><i></i><i></i><i></i></div>
      <nav class="docs-zz-side" data-penna="sidebar" data-ground aria-label="zz sessions">
        <div class="penna-strip docs-zz-strip">
          <button class="docs-zz-chrome" data-penna="chrome.button" type="button" aria-label="Session tree" title="Session tree"><i data-icon="chat"></i></button>
          <button class="docs-zz-chrome" data-penna="chrome.button" type="button" aria-label="Toggle sidebar" title="Toggle sidebar" data-sc-zz-side-toggle><i data-icon="panel-left"></i></button>
        </div>
        <div class="docs-zz-actions">
          <button class="docs-zz-action" data-penna="sidebar.row" type="button" data-sc-zz-new><i data-icon="chat-plus"></i><span data-penna-text>New chat</span></button>
          <button class="docs-zz-action" data-penna="sidebar.row" type="button"><i data-icon="server-plus"></i><span data-penna-text>Add host</span></button>
        </div>
        <div class="docs-zz-list">
          <div class="docs-zz-host"><i data-icon="laptop"></i><span data-penna-text>macbook</span></div>
          <div class="docs-zz-group" data-lit data-open data-sc-zz-group="hit">
            <div class="docs-zz-row" data-penna="row.nav" role="button" tabindex="0" data-sc-win="hit">
              <span class="docs-zz-stack" style="width: 33px">
                <span class="docs-zz-tile" data-penna="tile.stack" style="left: 0; z-index: 2"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="accent"></span></span>
                <span class="docs-zz-tile" data-penna="tile.stack" style="left: 7px; z-index: 1"></span>
              </span>
              <span class="docs-zz-copy">
                <span class="docs-zz-title">
                  <span class="docs-zz-label" data-penna-text>Fix stack hit tests</span>
                  <button class="docs-zz-chev-btn" type="button" aria-label="Toggle panes" aria-expanded="true" data-sc-zz-chev="hit"><i data-icon="chevron-right"></i></button>
                </span>
                <span class="docs-zz-sub"><span class="docs-zz-detail">penna · main</span><span class="docs-zz-age">now</span></span>
              </span>
            </div>
            <div class="docs-zz-nested" data-sc-zz-nested="hit">
              <div class="docs-zz-row docs-zz-nest" data-penna="row.nav" role="button" tabindex="0" data-state="current" aria-current="true" data-sc-win="hit" data-sc-focus="chat">
                <span class="docs-zz-lead"><span class="docs-zz-tile" data-penna="tile.stack"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="accent"></span></span></span>
                <span class="docs-zz-one"><span class="docs-zz-label" data-penna-text>Fix stack hit tests</span><span class="docs-zz-age">now</span></span>
              </div>
              <div class="docs-zz-row docs-zz-nest" data-penna="row.nav" role="button" tabindex="0" data-plain data-sc-win="hit" data-sc-focus="term">
                <span class="docs-zz-lead"><span class="docs-zz-tile" data-penna="tile.stack"><i data-icon="square-terminal"></i><span class="docs-zz-dot" data-tone="success"></span></span></span>
                <span class="docs-zz-one"><span class="docs-zz-label" data-penna-text>cargo test</span><span class="docs-zz-age">2m</span></span>
              </div>
            </div>
          </div>

          <div class="docs-zz-row" data-penna="row.nav" role="button" tabindex="0" data-sc-win="glass">
            <span class="docs-zz-stack" style="width: 33px">
              <span class="docs-zz-tile" data-penna="tile.stack" style="left: 0; z-index: 2"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="warning"></span></span>
            </span>
            <span class="docs-zz-copy">
              <span class="docs-zz-title"><span class="docs-zz-label" data-penna-text>Audit glass contrast</span></span>
              <span class="docs-zz-sub"><span class="docs-zz-detail"><span data-tone="warning">Needs approval</span> · rg</span><span class="docs-zz-age">4m</span></span>
            </span>
          </div>

          <div class="docs-zz-row" data-penna="row.nav" role="button" tabindex="0" data-plain data-sc-win="shell">
            <span class="docs-zz-stack" style="width: 33px">
              <span class="docs-zz-tile" data-penna="tile.stack" style="left: 0; z-index: 2"><i data-icon="square-terminal"></i></span>
            </span>
            <span class="docs-zz-copy">
              <span class="docs-zz-title"><span class="docs-zz-label" data-penna-text>fish</span></span>
              <span class="docs-zz-sub"><span class="docs-zz-detail">~/dev/penna · main</span><span class="docs-zz-age">12m</span></span>
            </span>
          </div>

          <div class="docs-zz-host"><i data-icon="server"></i><span data-penna-text>ubuntu</span></div>
          <div class="docs-zz-row" data-penna="row.nav" role="button" tabindex="0" data-sc-win="release">
            <span class="docs-zz-stack" style="width: 33px">
              <span class="docs-zz-tile" data-penna="tile.stack" style="left: 0; z-index: 2"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="success"></span></span>
            </span>
            <span class="docs-zz-copy">
              <span class="docs-zz-title"><span class="docs-zz-label" data-penna-text>Bump release notes</span></span>
              <span class="docs-zz-sub"><span class="docs-zz-detail"><span data-tone="normal">Done</span> · CHANGELOG</span><span class="docs-zz-age">1h</span></span>
            </span>
          </div>
        </div>
        <div class="docs-zz-foot">
          <button class="docs-zz-action" data-penna="sidebar.row" type="button"><i data-icon="settings"></i><span data-penna-text>Settings</span></button>
        </div>
      </nav>

      <header class="docs-zz-titlebar" data-penna="titlebar">
        <button class="docs-zz-chrome docs-zz-reopen" data-penna="chrome.button" type="button" aria-label="Show sidebar" title="Show sidebar" data-sc-zz-side-toggle><i data-icon="panel-top"></i></button>
        <div class="docs-zz-tabstrip" data-penna="tab.strip" role="tablist" aria-label="Open windows">
          <button class="docs-zz-tab" data-penna="tab.window" type="button" role="tab" aria-selected="true" data-state="current" data-sc-tab="hit"><i data-icon="sparkle"></i><span class="docs-zz-tab-label" data-penna-text>Fix stack hit tests</span></button>
          <button class="docs-zz-tab" data-penna="tab.window" type="button" role="tab" aria-selected="false" data-sc-tab="glass"><i data-icon="sparkle"></i><span class="docs-zz-tab-label" data-penna-text>Audit glass contrast</span></button>
          <button class="docs-zz-tab" data-penna="tab.window" type="button" role="tab" aria-selected="false" data-sc-tab="shell"><i data-icon="square-terminal"></i><span class="docs-zz-tab-label" data-penna-text>fish</span></button>
        </div>
      </header>

      <div class="docs-zz-panes" data-split="true" data-sc-zz-stage>
        <section class="docs-zz-pane" data-penna="pane" data-ground data-active data-sc-zz-pane-box="chat">
          <div class="docs-zz-head">
            <div class="docs-zz-bar" data-penna="bar.pane">
              <div class="docs-zz-pbtn" data-penna="button.title"><i data-icon="sparkle"></i><span class="docs-zz-pname" data-sc-zz-chat-name>Fix stack hit tests</span><span class="docs-zz-pcwd" data-sc-zz-chat-cwd>~/dev/penna</span></div>
              <span class="docs-zz-grow"></span>
              <div class="docs-zz-pacts">
                <button class="docs-zz-pact" data-penna="icon-button.bar" type="button" aria-label="Split right" data-sc-zz-split><i data-icon="panel-right"></i></button>
                <button class="docs-zz-pact" data-penna="icon-button.bar" type="button" aria-label="Float pane"><i data-icon="picture-in-picture"></i></button>
                <button class="docs-zz-pact" data-penna="icon-button.bar" type="button" aria-label="Close pane"><i data-icon="xmark"></i></button>
              </div>
            </div>
          </div>
          <div class="docs-zz-thread" data-sc-zz-feed></div>
          <form class="docs-zz-dock" data-sc-zz-form>
            <div class="penna-chat-composer">
              <div class="penna-chat-field input-chrome" data-penna="field.composer">
                <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Mention a file"><i data-icon="plus"></i></button>
                <input class="docs-zz-input" type="text" placeholder="Message agent…" aria-label="Message agent" data-sc-zz-input />
                <span class="penna-chat-meter" role="progressbar" aria-label="34% of context used" style="--penna-chat-used: 0.34"><span></span></span>
                <button class="penna-button" data-penna="button.send" data-shape="icon" data-emphasis="solid" data-solid="primary" type="submit" aria-label="Send message"><i data-icon="arrow-up"></i></button>
              </div>
            </div>
            <div class="docs-zz-dock-foot">
              <span class="docs-zz-grow"></span>
              <span class="penna-chip docs-zz-mchip" data-penna="chip" data-size="xsmall"><i data-icon="circle-user"></i><span class="penna-chip-label" data-penna-text>Ask</span></span>
              <span class="penna-chip docs-zz-mchip" data-penna="chip" data-size="xsmall"><span class="penna-chip-label" data-penna-text>Reasoning</span><span class="docs-zz-mchip-dim">High</span><i data-icon="chevron-down"></i></span>
            </div>
          </form>
        </section>

        <section class="docs-zz-pane" data-penna="pane" data-ground data-sc-zz-pane-box="term">
          <div class="docs-zz-head">
            <div class="docs-zz-bar" data-penna="bar.pane">
              <div class="docs-zz-pbtn" data-penna="button.title"><i data-icon="square-terminal"></i><span class="docs-zz-pname" data-sc-zz-term-name>cargo test</span><span class="docs-zz-pcwd" data-sc-zz-term-cwd>~/dev/penna</span></div>
              <span class="docs-zz-grow"></span>
              <div class="docs-zz-pacts">
                <button class="docs-zz-pact" data-penna="icon-button.bar" type="button" aria-label="Pane menu"><i data-icon="ellipsis"></i></button>
              </div>
            </div>
          </div>
          <pre class="docs-zz-term" data-sc-zz-out></pre>
        </section>
      </div>
    </div>
  </div>

  <div class="docs-sc-zz-phone" data-sc-zz-pane="phone" hidden>
    <div class="docs-phone docs-zz-phone">
      <div class="docs-zz-phone-scroll">
        <div class="penna-large-title">macbook</div>
        <div class="docs-zz-phone-sess">zebrilus</div>
        <div class="docs-zz-phone-col">
          <div class="docs-zz-pcard docs-zz-prow" data-penna="phone.card" data-warn role="button" tabindex="0">
            <span class="docs-zz-pface" data-penna="tile.stack"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="warning"></span></span>
            <span class="docs-zz-pcopy"><b>Audit glass contrast</b><span data-tone="warning">Wants to run cargo test</span></span>
            <i class="docs-zz-pchev" data-icon="chevron-right"></i>
          </div>

          <div class="docs-zz-pcard docs-zz-ptail" data-penna="phone.card" role="button" tabindex="0">
            <div class="docs-zz-ptail-head">
              <span class="docs-zz-pstack">
                <span class="docs-zz-pface docs-zz-pface-sm" data-penna="tile.stack"><i data-icon="square-terminal"></i></span>
                <span class="docs-zz-pface docs-zz-pface-sm" data-penna="tile.stack"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="accent"></span></span>
              </span>
              <b>cargo test</b>
            </div>
            <pre class="docs-zz-tail"><b>❯ cargo test --features tree-sitter</b>
running 493 tests
<span data-ink="success">test result: ok. 493 passed; 0 failed</span>
<b>❯ </b></pre>
          </div>

          <div class="docs-zz-pcard docs-zz-prow" data-penna="phone.card" role="button" tabindex="0">
            <span class="docs-zz-pface" data-penna="tile.stack"><i data-icon="sparkle"></i><span class="docs-zz-dot" data-tone="accent"></span></span>
            <span class="docs-zz-pcopy"><b>Fix stack hit tests</b><span>penna · main</span></span>
            <i class="docs-zz-pchev" data-icon="chevron-right"></i>
          </div>
        </div>
      </div>

      <div class="penna-compact-bar docs-sc-phone-bar">
        <button class="penna-touch-disc" data-penna="floating" type="button" aria-label="Machines"><i data-icon="chevron-up"></i></button>
        <div class="penna-compact-pill" data-penna="phone.pill" data-ground>
          <div class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="server"></i></div>
          <div class="penna-pill-copy">
            <span class="penna-pill-title">macbook</span>
            <span class="penna-pill-meta">
              <span class="penna-pill-meta-text">zebrilus</span>
              <span class="penna-dots"><span class="penna-dot" data-active></span><span class="penna-dot"></span><span class="penna-dot"></span></span>
            </span>
          </div>
        </div>
        <button class="penna-touch-disc" data-penna="floating" type="button" aria-label="New pane"><i data-icon="plus"></i></button>
      </div>
    </div>
  </div>
</div>
```

## nidus desk

Click any display tile on the arrangement canvas or a nearby host in the dock to inspect and toggle its live permissions.

```figure uses=chip,dock,icon,inspector,label,settings,switch,tile
<div class="docs-sc-nidus" data-sc-nidus>
  <div class="docs-sc-nidus-main">
    <div class="docs-sc-desk">
      <div class="docs-sc-desk-label"><span class="penna-type" data-type="caption" data-muted data-penna-text>Desk arrangement · QUIC mesh</span></div>
      <div class="docs-sc-desk-stage">
        <button class="penna-tile docs-sc-screen" data-penna="tile" data-variant="screen" type="button" data-sc-host="ubuntu">
          <span class="docs-sc-screen-name">ubuntu</span>
          <span class="docs-sc-screen-sub">27″ · 4K</span>
        </button>
        <button class="penna-tile docs-sc-screen docs-sc-screen-primary" data-penna="tile" data-variant="screen" data-state="selected" type="button" data-sc-host="macbook">
          <span class="docs-sc-screen-name">macbook</span>
          <span class="docs-sc-screen-sub">16″ · Primary</span>
        </button>
        <button class="penna-tile docs-sc-screen docs-sc-screen-sm" data-penna="tile" data-variant="screen" type="button" data-sc-host="thinkpad">
          <span class="docs-sc-screen-name">thinkpad</span>
          <span class="docs-sc-screen-sub">14″ · Linux</span>
        </button>
      </div>
    </div>
    <aside class="penna-inspector docs-sc-inspector" data-penna="inspector" data-ground>
      <div class="penna-inspector-header">
        <div class="penna-inspector-titles">
          <span class="penna-type" data-type="heading" data-truncate data-sc-nidus-name>macbook</span>
          <span class="penna-type" data-type="small" data-muted data-sc-nidus-sub>macOS · Primary · 1.2 ms</span>
        </div>
        <span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success" data-sc-nidus-dot></i><span class="penna-chip-label" data-sc-nidus-state>Online</span></span>
      </div>
      <div class="penna-settings-stack">
        <div class="penna-settings-group"><span data-penna-text>Permissions</span></div>
        <div class="penna-settings-list" role="list">
          <div class="penna-setting-entry" role="listitem">
            <div data-penna="settings.card" data-ground>
              <div class="penna-setting-body">
                <div class="penna-setting-heading">
                  <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Pointer &amp; keyboard</span></div></div>
                  <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Pointer and keyboard" type="button" data-sc-perm="pointer"></button>
                </div>
              </div>
            </div>
          </div>
          <div class="penna-setting-entry" role="listitem">
            <div data-penna="settings.card" data-ground>
              <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
              <div class="penna-setting-body">
                <div class="penna-setting-heading">
                  <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Clipboard sync</span></div></div>
                  <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Clipboard sync" type="button" data-sc-perm="clipboard"></button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="docs-sc-nidus-meta">
        <span class="penna-type" data-type="small" data-strong data-penna-text>Displays</span>
        <span class="penna-type" data-type="small" data-muted data-sc-nidus-displays>Built-in Liquid Retina 3456×2234</span>
      </div>
    </aside>
  </div>
  <div class="penna-dock docs-sc-dock">
    <div class="penna-dock-header">
      <div class="penna-dock-titles">
        <span class="penna-type" data-type="body" data-strong data-penna-text>Nearby</span>
        <span class="penna-type" data-type="small" data-muted>Discovered on local network via mDNS.</span>
      </div>
      <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>2 peers</span></span>
    </div>
    <div class="penna-dock-tiles">
      <button class="penna-tile" data-penna="tile.card" data-variant="card" type="button" data-sc-host="xps"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>xps</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Linux · 1 screen</span></button>
      <button class="penna-tile" data-penna="tile.card" data-variant="card" type="button" data-sc-host="studio"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>studio</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>macOS · 2 screens</span></button>
    </div>
  </div>
</div>
```

## Parity

Every component and block twin is rendered in both native `ala` GPU and headless Chromium across light and dark modes and scored pixel-for-pixel. The table below loads live from `assets/parity.json`, generated directly from `storybook/src/kit/*/parity.baseline.json`.

```figure uses=badge,input,progress,segmented
<div class="docs-sc-parity" data-sc-parity>
  <div class="docs-sc-kpis" data-sc-parity-kpis></div>
  <div class="docs-sc-parity-bar">
    <label class="penna-input docs-sc-parity-search" data-penna="field" data-size="small"><i data-icon="search"></i><input type="search" placeholder="Filter 34 stories…" aria-label="Filter parity stories" data-sc-parity-filter /></label>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Sort parity" data-sc-parity-sort>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Name</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Highest</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Lowest</span></span>
      </div>
    </div>
  </div>
  <div class="docs-sc-parity-list" data-penna="card" data-ground data-sc-parity-list></div>
</div>
```
