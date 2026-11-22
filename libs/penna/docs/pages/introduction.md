# Introduction

penna is the widget kit for [ala](https://github.com/autistic-computer/ala), built for zz and nidus. One set of tokens draws every surface, corner and rim on the GPU and in HTML.

```figure uses=menu,slider
<div class="docs-hero-stage" data-docs-hero>
  <canvas class="docs-hero-canvas" aria-hidden="true"></canvas>
  <div class="docs-hero-overlay">
    <div class="docs-hero-stack">
      <div class="penna-popup docs-hero-menu" data-penna="popover" data-ground>
        <div class="penna-popup-items" role="menu">
          <div class="penna-popup-row" data-penna="menu-item" role="menuitem" tabindex="0" data-hero-action="search">
            <span class="penna-popup-icon"><i data-icon="search"></i></span>
            <span class="penna-popup-main"><span class="penna-label" data-penna-text>Search docs</span><span class="penna-label penna-popup-key" data-penna-text data-hero-search-key>⌘K</span></span>
          </div>
          <div class="penna-popup-row" data-penna="menu-item" role="menuitem" tabindex="0" data-hero-action="foundations">
            <span class="penna-popup-icon"><i data-icon="palette"></i></span>
            <span class="penna-popup-main"><span class="penna-label" data-penna-text>Foundations</span></span>
          </div>
          <div class="penna-popup-row" data-penna="menu-item" role="menuitem" tabindex="0" data-hero-action="components">
            <span class="penna-popup-icon"><i data-icon="layers"></i></span>
            <span class="penna-popup-main"><span class="penna-label" data-penna-text>Components</span></span>
          </div>
          <div class="penna-popup-row" data-penna="menu-item" role="menuitem" tabindex="0" data-hero-action="look">
            <span class="penna-popup-icon"><i data-icon="sliders"></i></span>
            <span class="penna-popup-main"><span class="penna-label" data-penna-text>Look &amp; glass</span></span>
          </div>
          <div class="penna-popup-separator" role="separator"></div>
          <div class="penna-popup-row" data-penna="menu-item" role="menuitem" tabindex="0" data-hero-action="mode">
            <span class="penna-popup-icon"><i data-icon="sun" data-docs-mode-icon="dark"></i><i data-icon="moon" data-docs-mode-icon="light"></i></span>
            <span class="penna-popup-main"><span class="penna-label" data-penna-text data-docs-mode-icon="dark">Switch to light</span><span class="penna-label" data-penna-text data-docs-mode-icon="light">Switch to dark</span></span>
          </div>
        </div>
      </div>

      <div class="docs-hero-sky" data-penna="floating" data-ground>
        <i class="docs-hero-sky-icon" data-icon="sun" aria-hidden="true"></i>
        <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="56" data-hero-sky aria-label="Sky elevation">
          <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Two builds, one look

Most widgets have both a Rust version and a DOM version. Both read the same tokens, so a button in the app and a button on a web page come out the same. A parity runner renders each storybook page both ways and scores how close they are.

## How the source is laid out

| Folder | What lives there |
| --- | --- |
| `foundation` | Tokens, surface roles, motion and syntax highlighting |
| `components` | One widget each: button, input, kbd, switch, menu |
| `blocks` | Larger pieces made of components: settings, sidebar, title bar |

Each folder holds the Rust next to its CSS and JS, under the same name.

## One look

penna draws one look in light and dark. Adjust it with a few knobs (tint, corners, controls, glass, face) in Appearance, starting from the platform's defaults. Try Mode, Glass and Density from the header, or see [Look](look.html) for every knob.
