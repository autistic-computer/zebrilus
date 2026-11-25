# All blocks

Higher-level app patterns in `src/blocks` composed from penna's components — chrome rails, overlays, editors, agent chat, settings, and compact phone layouts.

```figure uses=appearance,badge,button,card,chat,chip,code_editor,color_picker,compact,dialog,diff,dock,feedback,icon,input,inspector,kbd,label,list,notice,notification,palette,panel,progress,segmented,settings,sheet,sidebar,stack_menu,steps,switch,table,tile,title_bar,touch,trace,which_key
<div class="docs-gal" data-docs-gallery>
  <div class="docs-gal-bar">
    <label class="penna-input docs-gal-search" data-penna="field" data-size="small"><i data-icon="search"></i><input type="search" placeholder="Filter 24 blocks…" aria-label="Filter blocks" data-gal-search /></label>
    <span class="docs-lk-mono" data-gal-count>24 of 24</span>
  </div>
  <div class="docs-gal-cats" role="group" aria-label="Block category">
    <button class="penna-chip" data-penna="chip" data-interactive data-state="selected" type="button" data-gal-cat="all"><span class="penna-chip-label" data-penna-text>All</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="chrome"><span class="penna-chip-label" data-penna-text>App chrome</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="workspace"><span class="penna-chip-label" data-penna-text>Workspace &amp; chat</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="overlay"><span class="penna-chip-label" data-penna-text>Overlays</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="status"><span class="penna-chip-label" data-penna-text>Status</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="touch"><span class="penna-chip-label" data-penna-text>Phone &amp; touch</span></button>
  </div>

  <div class="docs-gal-grid">
    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="action-bar.html" data-gal-item data-gal-cat="touch" data-gal-terms="action bar phone bottom cta button">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-button docs-gal-wide-btn" data-penna="button" data-tone="primary" data-prominent data-size="medium"><span class="penna-label">Connect</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Action bar</span><span class="docs-gal-desc">Bottom thumb-zone CTA bar for phone screens</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="appearance.html" data-gal-item data-gal-cat="workspace" data-gal-terms="appearance theme accent glass contrast">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-swatch" style="background: #3183ff"></span>
        <span class="docs-gal-swatch" style="background: #a366f7"></span>
        <span class="docs-gal-swatch" style="background: #e648a0"></span>
        <span class="docs-gal-swatch" style="background: #009c3a"></span>
        <span class="penna-toggle" aria-checked="true"></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Appearance</span><span class="docs-gal-desc">Shared Look, Size, and Accessibility settings screen</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="chat.html" data-gal-item data-gal-cat="workspace" data-gal-terms="chat agent composer thread messages">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-chat">
          <span class="docs-gal-bubble" data-penna="panel">cargo test · 473 ok</span>
          <span class="docs-gal-fake-input" data-penna="field"><span>Ask agent…</span><span class="docs-spacer"></span><i data-icon="arrow-up"></i></span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Chat</span><span class="docs-gal-desc">Agent transcript, tool activity, and composer</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="code-editor.html" data-gal-item data-gal-cat="workspace" data-gal-terms="code editor syntax tree-sitter gutter">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-code" data-penna="card.code"><span><b>fn</b> main() {</span><span>&nbsp;&nbsp;penna::init();</span><span>}</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Code editor</span><span class="docs-gal-desc">Syntax-highlighted code block with line gutter</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="color-picker.html" data-gal-item data-gal-cat="workspace" data-gal-terms="color picker oklch hue hex swatch">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-color-bar"></div>
        <span class="docs-lk-mono">#3183ff</span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Color picker</span><span class="docs-gal-desc">OKLCH saturation square, hue rail, and hex input</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="dialog.html" data-gal-item data-gal-cat="overlay" data-gal-terms="dialog modal confirm alert prompt">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground>
          <span class="penna-type" data-type="small" data-strong>Close window?</span>
          <div class="docs-row" style="justify-content: flex-end"><span class="penna-button" data-penna="button" data-size="xsmall"><span class="penna-label">Cancel</span></span><span class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="xsmall"><span class="penna-label">Close</span></span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Dialog</span><span class="docs-gal-desc">Modal confirmation and focused form prompt</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="dock.html" data-gal-item data-gal-cat="chrome" data-gal-terms="dock tray nearby hosts tiles">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-dock">
          <span class="penna-tile docs-gal-sm-tile" data-penna="tile.card">xps</span>
          <span class="penna-tile docs-gal-sm-tile" data-penna="tile.card" data-state="selected">thinkpad</span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Dock</span><span class="docs-gal-desc">Bottom tray holding selectable host and tile cards</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="empty-state.html" data-gal-item data-gal-cat="status" data-gal-terms="empty state placeholder zero feedback">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-col" style="align-items: center"><i data-icon="inbox" data-tone="muted"></i><span class="penna-type" data-type="small" data-strong>No sessions yet</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Empty state</span><span class="docs-gal-desc">Zero-state guidance with icon, copy, and primary CTA</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="inspector.html" data-gal-item data-gal-cat="chrome" data-gal-terms="inspector panel properties detail sidebar">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="panel" data-ground>
          <div class="docs-row"><span class="penna-type" data-type="small" data-strong>ubuntu</span><span class="docs-spacer"></span><i class="penna-dot" data-tone="success"></i></div>
          <span class="penna-type" data-type="caption" data-muted>2 screens · 0.9 ms</span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Inspector</span><span class="docs-gal-desc">Trailing side panel for selected item details</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="notice.html" data-gal-item data-gal-cat="status" data-gal-terms="notice banner callout alert info warning">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-chip" data-penna="chip" data-tone="warning"><i data-icon="triangle-alert"></i><span class="penna-chip-label">Reconnect required</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Notice</span><span class="docs-gal-desc">Inline tonal callout for warnings, errors, and hints</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="notification.html" data-gal-item data-gal-cat="status" data-gal-terms="notification toast alert stack">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground>
          <div class="docs-row"><i data-icon="circle-check" data-tone="success"></i><span class="penna-type" data-type="small" data-strong>Saved theme</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Notification</span><span class="docs-gal-desc">Glass toast notifications with action and expand</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="palette.html" data-gal-item data-gal-cat="overlay" data-gal-terms="palette command search launcher cmd-k">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground>
          <div class="docs-row"><i data-icon="search" data-tone="muted"></i><span class="penna-type" data-type="small" data-muted>Jump to…</span><span class="docs-spacer"></span><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-k"></kbd></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Palette</span><span class="docs-gal-desc">Fuzzy command launcher with keyboard hints</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="settings.html" data-gal-item data-gal-cat="workspace" data-gal-terms="settings preferences grouped rows">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="settings.card" data-ground>
          <div class="docs-row"><span class="penna-type" data-type="small">Auto-reconnect</span><span class="docs-spacer"></span><span class="penna-toggle" aria-checked="true"></span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Settings</span><span class="docs-gal-desc">Grouped preference rows with controls and copy</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="sheet.html" data-gal-item data-gal-cat="overlay" data-gal-terms="sheet drawer bottom phone slide">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-sheet" data-penna="popover" data-ground><span class="docs-gal-grabber"></span><span class="penna-type" data-type="caption" data-muted>Choose action</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Sheet</span><span class="docs-gal-desc">Modal bottom sheet with grabber for touch picks</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="sidebar.html" data-gal-item data-gal-cat="chrome" data-gal-terms="sidebar rail navigation window">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-list">
          <div class="penna-sidebar-row" data-penna="sidebar.row" data-state="current" style="height: 24px"><span class="penna-label">Workspaces</span></div>
          <div class="penna-sidebar-row" data-penna="sidebar.row" style="height: 24px"><span class="penna-label">Machines</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Sidebar</span><span class="docs-gal-desc">Navigation rail with glass current row and sections</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="stack-menu.html" data-gal-item data-gal-cat="touch" data-gal-terms="stack menu phone fab disc floating">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-stack-mini">
          <span class="penna-chip" data-penna="popover"><i data-icon="square-terminal"></i><span class="penna-chip-label">Terminal</span></span>
          <span class="penna-chip" data-penna="popover"><i data-icon="chat"></i><span class="penna-chip-label">Chat</span></span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Stack menu</span><span class="docs-gal-desc">Floating glass action stack rising from a touch disc</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="steps.html" data-gal-item data-gal-cat="status" data-gal-terms="steps stepper wizard progress flow">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-steps">
          <span class="docs-gal-step" data-done><i data-icon="check"></i></span>
          <span class="docs-gal-step-line" data-done></span>
          <span class="docs-gal-step" data-current>2</span>
          <span class="docs-gal-step-line"></span>
          <span class="docs-gal-step">3</span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Steps</span><span class="docs-gal-desc">Multi-step setup and workflow indicator</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="title-bar.html" data-gal-item data-gal-cat="chrome" data-gal-terms="title bar window lights controls chrome">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-bar" data-penna="panel" data-ground>
          <span class="docs-gal-lights"><i></i><i></i><i></i></span>
          <span class="penna-type" data-type="caption">studio</span>
          <span class="docs-spacer"></span>
          <i data-icon="panel-right"></i>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Title bar</span><span class="docs-gal-desc">Window header with platform lights and controls</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="ui-block.html" data-gal-item data-gal-cat="workspace" data-gal-terms="ui block a2ui agent surface json">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="card" data-ground>
          <div class="docs-row"><i data-icon="braces" data-tone="accent"></i><span class="penna-type" data-type="small" data-strong>Agent surface</span></div>
          <span class="penna-type" data-type="caption" data-muted>Structured JSON → native UI</span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">UI block</span><span class="docs-gal-desc">Declarative agent-rendered interface surfaces</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="which-key.html" data-gal-item data-gal-cat="overlay" data-gal-terms="which key shortcuts cheat sheet chords">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground>
          <div class="docs-row"><kbd class="penna-kbd" data-penna="keycap" data-size="small">s</kbd><span class="penna-type" data-type="caption">Split right</span></div>
          <div class="docs-row"><kbd class="penna-kbd" data-penna="keycap" data-size="small">z</kbd><span class="penna-type" data-type="caption">Zoom pane</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Which key</span><span class="docs-gal-desc">Multi-column leader-key shortcut guide</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="zoom-card.html" data-gal-item data-gal-cat="touch" data-gal-terms="zoom card phone machine detail">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="card" data-ground>
          <div class="docs-row"><span class="penna-type" data-type="small" data-strong>Add machine</span><span class="docs-spacer"></span><i data-icon="xmark"></i></div>
          <span class="penna-type" data-type="caption" data-muted>3 found on tailnet</span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Zoom card</span><span class="docs-gal-desc">Full-screen phone view expanded from a tile</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="diff.html" data-gal-item data-gal-cat="workspace" data-gal-terms="diff view stat unified split git code">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-diff-stat"><span class="penna-diff-add">+42</span><span class="penna-diff-rem">−18</span><span class="penna-diff-pips"><i data-kind="add"></i><i data-kind="add"></i><i data-kind="add"></i><i data-kind="rem"></i><i data-kind="rem"></i></span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Diff</span><span class="docs-gal-desc">Unified and split code diffs with five-block DiffStat</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="table.html" data-gal-item data-gal-cat="workspace" data-gal-terms="table grid sortable data columns rows">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="card" data-ground style="gap: 4px">
          <div class="docs-row"><span class="penna-type" data-type="caption" data-muted>Session</span><span class="docs-spacer"></span><span class="penna-type" data-type="caption" data-muted>RTT</span></div>
          <div class="docs-row"><span class="penna-type" data-type="small" data-strong>work</span><span class="docs-spacer"></span><span class="docs-lk-mono">1.2 ms</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Table</span><span class="docs-gal-desc">Density-aware sortable data table with row selection</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="trace.html" data-gal-item data-gal-cat="workspace" data-gal-terms="trace agent tool calls timeline execution">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="card" data-ground style="gap: 4px">
          <div class="docs-row"><i data-icon="circle-check" data-tone="success"></i><span class="docs-lk-mono">cargo test</span><span class="docs-spacer"></span><span class="penna-type" data-type="caption" data-muted>980 ms</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Trace</span><span class="docs-gal-desc">Collapsible agent tool-call timeline with output wells</span></div>
    </a>
  </div>
</div>
```
