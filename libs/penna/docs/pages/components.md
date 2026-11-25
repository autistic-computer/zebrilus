# All components

Every primitive component in `src/components`, rendered live with the active Look, Mode, Density, and Glass tokens. Filter by name or category, or pick any card to open its full guide.

```figure uses=badge,bar,breadcrumb,button,button_group,card,chart,check,chip,combobox,data_list,disclosure,field,hover_card,icon,input,input_group,kbd,key_field,label,list,menu,meter,panel,popover,progress,row,scroll,segmented,select,separator,skeleton,slider,snippet,spinner,split,status_dot,switch,tab,text,tile,tooltip,tree,well
<div class="docs-gal" data-docs-gallery>
  <div class="docs-gal-bar">
    <label class="penna-input docs-gal-search" data-penna="field" data-size="small"><i data-icon="search"></i><input type="search" placeholder="Filter 44 components…" aria-label="Filter components" data-gal-search /></label>
    <span class="docs-lk-mono" data-gal-count>44 of 44</span>
  </div>
  <div class="docs-gal-cats" role="group" aria-label="Component category">
    <button class="penna-chip" data-penna="chip" data-interactive data-state="selected" type="button" data-gal-cat="all"><span class="penna-chip-label" data-penna-text>All</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="actions"><span class="penna-chip-label" data-penna-text>Actions</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="inputs"><span class="penna-chip-label" data-penna-text>Inputs</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="display"><span class="penna-chip-label" data-penna-text>Display</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="navigation"><span class="penna-chip-label" data-penna-text>Navigation</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="layout"><span class="penna-chip-label" data-penna-text>Layout</span></button>
    <button class="penna-chip" data-penna="chip" data-interactive type="button" data-gal-cat="overlay"><span class="penna-chip-label" data-penna-text>Overlay</span></button>
  </div>

  <div class="docs-gal-grid">
    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="button.html" data-gal-item data-gal-cat="actions" data-gal-terms="button primary quiet outline icon action">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-button" data-penna="button" data-size="small"><span class="penna-label">Cancel</span></span>
        <span class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small"><span class="penna-label">Save</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Button</span><span class="docs-gal-desc">Primary, default, quiet, outline, and icon actions</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="badge.html" data-gal-item data-gal-cat="display" data-gal-terms="badge count status tone pill">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-badge" data-penna="badge"><span class="badge-text">12</span></span>
        <span class="penna-badge" data-penna="badge" data-tone="success"><i data-icon="circle-check"></i><span class="badge-text">Ready</span></span>
        <span class="penna-badge" data-penna="badge" data-tone="danger"><span class="badge-text">3</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Badge</span><span class="docs-gal-desc">Compact status counts and semantic tones</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="bar.html" data-gal-item data-gal-cat="layout" data-gal-terms="bar toolbar header footer strip">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-bar" data-penna="panel" data-ground><i data-icon="panel-left"></i><span>workspace</span><span class="docs-spacer"></span><i data-icon="plus"></i></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Bar</span><span class="docs-gal-desc">Horizontal strip for chrome controls and actions</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="card.html" data-gal-item data-gal-cat="layout" data-gal-terms="card container surface group">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="card" data-ground><span class="penna-type" data-type="small" data-strong>Session</span><span class="penna-type" data-type="caption" data-muted>3 active panes</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Card</span><span class="docs-gal-desc">Stepped surface container with superellipse corners</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="chart.html" data-gal-item data-gal-cat="display" data-gal-terms="chart sparkline bars graph telemetry">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-bars"><i style="height: 36%"></i><i style="height: 62%"></i><i style="height: 48%"></i><i style="height: 88%"></i><i style="height: 70%"></i><i style="height: 94%"></i><i style="height: 58%"></i></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Chart</span><span class="docs-gal-desc">Sparklines and bar plots on kit tokens</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="check.html" data-gal-item data-gal-cat="inputs" data-gal-terms="check checkbox radio boolean">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label">Auto-save</span></span>
        <span class="penna-checkbox" aria-checked="false"><span class="penna-check"></span><span class="penna-label">Wrap</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Check</span><span class="docs-gal-desc">Checkboxes and radios with accent-tinted state</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="chip.html" data-gal-item data-gal-cat="display" data-gal-terms="chip tag filter dot status">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-chip-label">Online</span></span>
        <span class="penna-chip" data-penna="chip"><i data-icon="git-branch"></i><span class="penna-chip-label">main</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Chip</span><span class="docs-gal-desc">Tonal pills with dots, icons, or dismiss actions</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="field.html" data-gal-item data-gal-cat="inputs" data-gal-terms="field form label helper error">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-field"><span class="penna-type" data-type="caption" data-muted>Host</span><span class="docs-gal-fake-input" data-penna="field">studio.local</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Field</span><span class="docs-gal-desc">Labeled input wrapper with helper and error copy</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="icon.html" data-gal-item data-gal-cat="display" data-gal-terms="icon glyph svg solar 107">
      <div class="docs-gal-stage" aria-hidden="true">
        <i data-icon="square-terminal" data-size="large"></i>
        <i data-icon="git-branch" data-size="large"></i>
        <i data-icon="sliders" data-size="large"></i>
        <i data-icon="sparkle" data-size="large" data-tone="accent"></i>
        <i data-icon="laptop" data-size="large"></i>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Icon</span><span class="docs-gal-desc">107 theme-adaptive glyphs on a 24 px grid</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="input.html" data-gal-item data-gal-cat="inputs" data-gal-terms="input text search multiline field">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-fake-input" data-penna="field"><i data-icon="search"></i><span>Filter sessions…</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Input</span><span class="docs-gal-desc">Single-line and multiline text entry fields</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="kbd.html" data-gal-item data-gal-cat="display" data-gal-terms="kbd keycap shortcut hotkey key">
      <div class="docs-gal-stage" aria-hidden="true">
        <kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd>
        <kbd class="penna-kbd" data-penna="keycap" data-kbd="shift-enter"></kbd>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Kbd</span><span class="docs-gal-desc">Platform-aware keycaps and chord glyphs</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="key-field.html" data-gal-item data-gal-cat="inputs" data-gal-terms="key field keybinding shortcut recorder">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-fake-input" data-penna="field" data-state="focused"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-k"></kbd><span class="docs-spacer"></span><i data-icon="xmark"></i></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Key field</span><span class="docs-gal-desc">Shortcut recorder for binding key chords</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="label.html" data-gal-item data-gal-cat="display" data-gal-terms="label typography type text heading">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-col"><span class="penna-type" data-type="heading">Heading 17</span><span class="penna-type" data-type="caption" data-muted>Caption 11 · tabular</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Label</span><span class="docs-gal-desc">Typed text styles across interface, reading, and touch</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="list.html" data-gal-item data-gal-cat="navigation" data-gal-terms="list rows listbox selection">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-list">
          <div class="penna-list-item" data-penna="menu-item" data-state="selected"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label">work</span></span></div>
          <div class="penna-list-item" data-penna="menu-item"><span class="penna-list-content"><i class="penna-list-icon" data-icon="folder"></i><span class="penna-list-label">logs</span></span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">List</span><span class="docs-gal-desc">Selectable rows with single neutral highlight</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="menu.html" data-gal-item data-gal-cat="overlay" data-gal-terms="menu context dropdown popup">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-popup docs-gal-mini-pop" data-penna="popover" data-ground>
          <div class="penna-popup-items">
            <div class="penna-popup-row" data-penna="menu-item" data-state="selected"><span class="penna-popup-main"><span class="penna-label">Split right</span></span></div>
            <div class="penna-popup-row" data-penna="menu-item"><span class="penna-popup-main"><span class="penna-label">Close pane</span></span></div>
          </div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Menu</span><span class="docs-gal-desc">Glass context and action menus with submenus</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="panel.html" data-gal-item data-gal-cat="layout" data-gal-terms="panel container ground surface">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-panel" data-penna="panel" data-ground><span class="penna-type" data-type="caption" data-muted>Panel ground</span><span class="penna-button" data-penna="button" data-size="xsmall"><span class="penna-label">Action</span></span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Panel</span><span class="docs-gal-desc">Grouped chrome panel with automatic ground propagation</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="popover.html" data-gal-item data-gal-cat="overlay" data-gal-terms="popover floating glass anchor">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground><span class="penna-type" data-type="small" data-strong>Appearance</span><span class="penna-type" data-type="caption" data-muted>Floating glass surface</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Popover</span><span class="docs-gal-desc">Anchored floating surface with liquid glass</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="progress.html" data-gal-item data-gal-cat="display" data-gal-terms="progress bar loading determinate">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-progress" style="max-width: 168px"><div class="penna-progress-fill" style="--penna-value: 68%"></div></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Progress</span><span class="docs-gal-desc">Determinate and indeterminate progress tracks</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="row.html" data-gal-item data-gal-cat="layout" data-gal-terms="row item horizontal detail">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-row" data-penna="row"><i data-icon="server"></i><span>studio.local</span><span class="docs-spacer"></span><span class="penna-type" data-type="caption" data-muted>1.2 ms</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Row</span><span class="docs-gal-desc">Density-aware row primitive with leading and trailing slots</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="scroll.html" data-gal-item data-gal-cat="layout" data-gal-terms="scroll scrollbar overflow fade">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-scroll-box" data-penna="panel" data-ground>
          <div class="docs-gal-scroll-lines"><i></i><i></i><i></i></div>
          <span class="docs-gal-scroll-thumb"></span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Scroll</span><span class="docs-gal-desc">Overlay scrollbars with inset tracks and quiet thumbs</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="segmented.html" data-gal-item data-gal-cat="inputs" data-gal-terms="segmented control tabs radio pill">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0">
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" aria-checked="true"><span class="penna-label">Day</span></span>
            <span class="penna-segment" aria-checked="false"><span class="penna-label">Week</span></span>
          </div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Segmented</span><span class="docs-gal-desc">Concentric pill picker for 2–4 exclusive choices</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="select.html" data-gal-item data-gal-cat="inputs" data-gal-terms="select dropdown picker combobox">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-fake-input" data-penna="field"><span>Regular</span><span class="docs-spacer"></span><i data-icon="chevron-down"></i></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Select</span><span class="docs-gal-desc">Popup menu picker for longer option lists</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="separator.html" data-gal-item data-gal-cat="layout" data-gal-terms="separator rule divider hairline">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-sep-demo"><span class="penna-type" data-type="caption" data-muted>Left</span><span class="docs-gal-sep-line"></span><span class="penna-type" data-type="caption" data-muted>Right</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Separator</span><span class="docs-gal-desc">Hairline rules for panes, lists, and sections</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="slider.html" data-gal-item data-gal-cat="inputs" data-gal-terms="slider range track knob">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="68" style="width: 156px">
          <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Slider</span><span class="docs-gal-desc">Accent-filled continuous track and drag thumb</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="spinner.html" data-gal-item data-gal-cat="display" data-gal-terms="spinner loader busy loading">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-spinner" data-size="large"><i data-icon="loader"></i></span>
        <span class="penna-spinner" data-size="large" data-color="accent"><i data-icon="loader"></i></span>
        <span class="penna-spinner" data-size="large" data-color="muted"><i data-icon="loader"></i></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Spinner</span><span class="docs-gal-desc">900 ms linear loader respecting reduced motion</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="switch.html" data-gal-item data-gal-cat="inputs" data-gal-terms="switch toggle boolean on off">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-toggle" aria-checked="true"></span>
        <span class="penna-toggle" aria-checked="false"></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Switch</span><span class="docs-gal-desc">Immediate binary toggle with accent track</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="tab.html" data-gal-item data-gal-cat="navigation" data-gal-terms="tab strip window page tabs">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-tab-strip" data-penna="panel">
          <span class="penna-tab" data-penna="tab.page" data-size="small" data-state="current" data-still><span class="penna-tab-label">work</span></span>
          <span class="penna-tab" data-penna="tab.page" data-size="small" data-still><span class="penna-tab-label">logs</span></span>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Tab</span><span class="docs-gal-desc">Window and page tab strips with glass active state</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="text.html" data-gal-item data-gal-cat="display" data-gal-terms="text markdown prose code syntax">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-col"><span class="penna-type" data-type="body"><strong>Markdown</strong> with <code data-penna="card.code">inline</code></span><span class="penna-type" data-type="caption" data-muted>640 px reading measure</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Text</span><span class="docs-gal-desc">Markdown prose, tables, and syntax-highlighted code</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="tile.html" data-gal-item data-gal-cat="layout" data-gal-terms="tile screen machine card">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-tile docs-gal-mini-tile" data-penna="tile.card" data-variant="card" data-state="selected"><span class="penna-type" data-type="small" data-strong>macbook</span><span class="penna-type" data-type="caption" data-muted>2 panes</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Tile</span><span class="docs-gal-desc">Selectable screen and machine cards for canvases</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="tooltip.html" data-gal-item data-gal-cat="overlay" data-gal-terms="tooltip hint shortcut hover">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-tooltip docs-lk-tooltip" data-penna="tooltip"><span>Split right</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Tooltip</span><span class="docs-gal-desc">Compact floating hint for icon buttons and shortcuts</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="tree.html" data-gal-item data-gal-cat="navigation" data-gal-terms="tree hierarchy files folder disclosure">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-list">
          <div class="penna-tree-row" data-penna="menu-item" style="--depth: 0; height: 24px"><span class="penna-tree-content"><i data-icon="chevron-down"></i><span class="penna-type" data-type="small">src</span></span></div>
          <div class="penna-tree-row" data-penna="menu-item" data-state="selected" style="--depth: 1; height: 24px"><span class="penna-tree-content"><i data-icon="file"></i><span class="penna-type" data-type="small">motion.rs</span></span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Tree</span><span class="docs-gal-desc">Indented hierarchical outline with elbow guides</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="well.html" data-gal-item data-gal-cat="layout" data-gal-terms="well recessed container inset">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-well" data-penna="well"><span class="penna-type" data-type="caption" data-muted>Recessed well</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Well</span><span class="docs-gal-desc">Sunken container for previews and drop targets</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="breadcrumb.html" data-gal-item data-gal-cat="navigation" data-gal-terms="breadcrumb path trail navigation">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-breadcrumb"><span class="penna-breadcrumb-item">penna</span><span class="penna-breadcrumb-sep">/</span><span class="penna-breadcrumb-item" aria-current="page">motion.rs</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Breadcrumb</span><span class="docs-gal-desc">Horizontal path trail for nested workspaces</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="button-group.html" data-gal-item data-gal-cat="actions" data-gal-terms="button group attached cluster toolbar">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-button-group" data-penna="field"><span class="penna-button" data-penna="button.quiet" data-size="xsmall"><span class="penna-label">Left</span></span><span class="penna-button" data-penna="button.quiet" data-size="xsmall"><span class="penna-label">Right</span></span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Button group</span><span class="docs-gal-desc">Attached action cluster sharing one container</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="combobox.html" data-gal-item data-gal-cat="inputs" data-gal-terms="combobox filter tokens multi select">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-fake-input" data-penna="field"><span class="penna-combobox-token">host:ubuntu</span><span class="docs-spacer"></span><i data-icon="chevron-down"></i></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Combobox</span><span class="docs-gal-desc">Filterable multi-token and searchable picker</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="data-list.html" data-gal-item data-gal-cat="display" data-gal-terms="data list metadata key value inspector">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-list" style="gap: 4px">
          <div class="docs-row"><span class="penna-type" data-type="caption" data-muted>Host</span><span class="docs-spacer"></span><span class="docs-lk-mono">ubuntu</span></div>
          <div class="docs-row"><span class="penna-type" data-type="caption" data-muted>RTT</span><span class="docs-spacer"></span><span class="docs-lk-mono">0.9 ms</span></div>
        </div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Data list</span><span class="docs-gal-desc">Structured key–value metadata rows</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="disclosure.html" data-gal-item data-gal-cat="layout" data-gal-terms="disclosure collapsible expand chevron">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-row" data-penna="menu-item"><i data-icon="chevron-down"></i><span>Build env</span><span class="docs-spacer"></span><span class="penna-type" data-type="caption" data-muted>3 vars</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Disclosure</span><span class="docs-gal-desc">Collapsible section with animated chevron turn</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="hover-card.html" data-gal-item data-gal-cat="overlay" data-gal-terms="hover card preview tooltip floating">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-card" data-penna="popover" data-ground><span class="penna-type" data-type="small" data-strong>e5c2c7e · main</span><span class="penna-type" data-type="caption" data-muted>Delayed hover preview</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Hover card</span><span class="docs-gal-desc">Delayed rich floating preview on hover or focus</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="input-group.html" data-gal-item data-gal-cat="inputs" data-gal-terms="input group prefix suffix addon">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="docs-gal-fake-input" data-penna="field"><span class="docs-lk-mono">zz://</span><span>studio</span><span class="docs-spacer"></span><span class="docs-lk-mono">:7443</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Input group</span><span class="docs-gal-desc">Field joined with leading and trailing addons</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="meter.html" data-gal-item data-gal-cat="display" data-gal-terms="meter gauge quota threshold capacity">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-meter" data-tone="warning" style="max-width: 176px"><div class="penna-meter-head"><span class="penna-meter-label">Context</span><span class="penna-meter-value">79%</span></div><div class="penna-meter-track"><div class="penna-meter-fill" style="--penna-value: 79%"></div></div></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Meter</span><span class="docs-gal-desc">Scalar capacity gauge with threshold tones</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="skeleton.html" data-gal-item data-gal-cat="display" data-gal-terms="skeleton shimmer loading placeholder">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-list" style="gap: 6px"><span class="penna-skeleton" style="width: 72%"></span><span class="penna-skeleton" style="width: 48%; height: 10px"></span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Skeleton</span><span class="docs-gal-desc">Layout-preserving shimmer loading placeholders</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="snippet.html" data-gal-item data-gal-cat="actions" data-gal-terms="snippet copy command cli code">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="penna-snippet" data-penna="card.code" style="max-width: 196px"><span class="penna-snippet-prefix">$</span><span class="penna-snippet-code">cargo test</span><i data-icon="copy"></i></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Snippet</span><span class="docs-gal-desc">Copyable command and address strip</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="split.html" data-gal-item data-gal-cat="layout" data-gal-terms="split panes sash resize divider">
      <div class="docs-gal-stage" aria-hidden="true">
        <div class="docs-gal-mini-panel" data-penna="panel" data-ground style="gap: 8px; height: 54px"><span class="penna-type" data-type="caption" data-muted>42%</span><span style="width: 2px; height: 34px; border-radius: 2px; background: var(--penna-accent)"></span><span class="penna-type" data-type="caption" data-muted>58%</span></div>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Split</span><span class="docs-gal-desc">Resizable two-pane container with 1:1 sash drag</span></div>
    </a>

    <a class="docs-gal-card penna-card" data-penna="card" data-ground href="status-dot.html" data-gal-item data-gal-cat="display" data-gal-terms="status dot online live pulse indicator">
      <div class="docs-gal-stage" aria-hidden="true">
        <span class="penna-status-dot" data-tone="success" data-pulse><span class="penna-status-dot-mark"></span><span>Online</span></span>
        <span class="penna-status-dot" data-tone="warning"><span class="penna-status-dot-mark"></span><span>Sync</span></span>
      </div>
      <div class="docs-gal-foot"><span class="docs-gal-name">Status dot</span><span class="docs-gal-desc">Live semantic indicator with optional pulse</span></div>
    </a>
  </div>
</div>
```
