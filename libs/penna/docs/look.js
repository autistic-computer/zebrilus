(() => {
  if (!document.querySelector("[data-docs-look]")) return;

  const ACCENTS = {
    default: { name: "blue", label: "Blue", light: "#0a64dd", dark: "#3183ff" },
    purple: { name: "purple", label: "Purple", light: "#8549d1", dark: "#a366f7" },
    pink: { name: "pink", label: "Pink", light: "#c02a82", dark: "#e648a0" },
    red: { name: "red", label: "Red", light: "#cc252d", dark: "#f44647" },
    orange: { name: "orange", label: "Orange", light: "#ad4f00", dark: "#da6500" },
    yellow: { name: "yellow", label: "Yellow", light: "#876400", dark: "#ac8000" },
    green: { name: "green", label: "Green", light: "#007b2c", dark: "#009c3a" },
    graphite: { name: "graphite", label: "Graphite", light: "#6a6a6a", dark: "#868686" },
  };

  const PALETTES = [
    {
      look: "default",
      meta: { light: "bg #f6f6f3 · fg #1c1c1a", dark: "bg #1a1a19 · fg #e9e7e3" },
      note: "Default kit palette; neutral warm 60 at 0.15.",
    },
    {
      look: "colors-slate",
      meta: { light: "bg #f2f5fa · fg #1a2230", dark: "bg #151a22 · fg #dce4f0" },
      note: "Cool slate base; surfaces, washes and hairlines derive from bg and fg.",
    },
    {
      look: "colors-warm",
      meta: { light: "bg #fbf1c7 · fg #3c3836", dark: "bg #1d2021 · fg fitted to 7:1" },
      note: "Dim fg and red danger lift in OKLCH lightness until they clear 7:1 and 4.5:1.",
    },
  ];

  const SMOOTH_LEVELS = [2, 3, 4, 5];
  const SMOOTH_NOTES = {
    0: "All four pass through the same 45° midpoint; smoother corners start earlier along each edge.",
    2: "smoothing 2 · 1.00× box · 24.0 px reach · plain circular arc",
    3: "smoothing 3 · 1.42× box · 34.1 px reach · kit default",
    4: "smoothing 4 · 1.84× box · 44.2 px reach · Apple default",
    5: "smoothing 5 · 2.26× box · 54.3 px reach · deepest squircle",
  };

  const SVG_NS = "http://www.w3.org/2000/svg";

  function mode() {
    return document.documentElement.dataset.pennaMode === "light" ? "light" : "dark";
  }

  document.querySelectorAll("[data-look-kind-group]").forEach((group) => {
    const attr = group.dataset.lookKindAttr || "selected";
    const items = [...group.querySelectorAll("[data-look-kind-item]")];
    items.forEach((item) => {
      item.addEventListener("click", () => {
        items.forEach((other) => {
          const active = other === item;
          if (active) other.dataset.state = attr;
          else delete other.dataset.state;
          if (attr === "current") {
            if (active) other.setAttribute("aria-current", "page");
            else other.removeAttribute("aria-current");
          } else {
            other.setAttribute("aria-selected", String(active));
          }
        });
      });
    });
  });

  const tintRoot = document.querySelector("[data-look-tint]");
  let pickedTint = "default";

  function syncTint() {
    if (!tintRoot) return;
    const m = mode();
    const stage = tintRoot.querySelector("[data-look-tint-stage]");
    const readout = tintRoot.querySelector("[data-look-tint-readout]");
    const entry = ACCENTS[pickedTint] || ACCENTS.default;
    if (stage) {
      stage.dataset.look = pickedTint;
      window.pennaKit?.roundAll(stage);
    }
    if (readout) readout.textContent = `${entry.name} · ${entry[m]}`;
    tintRoot.querySelectorAll("[data-look-accent]").forEach((btn) => {
      const key = btn.dataset.lookAccent;
      const info = ACCENTS[key];
      if (info) btn.style.setProperty("--overview-swatch", info[m]);
      const active = key === pickedTint;
      btn.setAttribute("aria-pressed", String(active));
      btn.closest(".docs-ov-ring")?.toggleAttribute("data-selected", active);
    });
  }

  if (tintRoot) {
    tintRoot.querySelectorAll("[data-look-accent]").forEach((btn) => {
      btn.addEventListener("click", () => {
        pickedTint = btn.dataset.lookAccent || "default";
        syncTint();
      });
    });
    const rows = [...tintRoot.querySelectorAll("[data-look-tint-row]")];
    rows.forEach((row) => {
      row.addEventListener("click", () => {
        rows.forEach((other) => {
          const active = other === row;
          if (active) other.dataset.state = "selected";
          else delete other.dataset.state;
          other.setAttribute("aria-selected", String(active));
        });
      });
    });
    syncTint();
  }

  const colorsRoot = document.querySelector("[data-look-colors]");
  let pickedPalette = 0;

  function syncColors() {
    if (!colorsRoot) return;
    const m = mode();
    const spec = PALETTES[pickedPalette] || PALETTES[0];
    const stage = colorsRoot.querySelector("[data-look-colors-stage]");
    const meta = colorsRoot.querySelector("[data-look-colors-meta]");
    const note = colorsRoot.querySelector("[data-look-colors-note]");
    if (stage) {
      stage.dataset.look = spec.look;
      requestAnimationFrame(() => {
        window.pennaKit?.roundAll(stage);
        window.pennaSegmented?.wire(stage);
      });
    }
    if (meta) meta.textContent = spec.meta[m];
    if (note) note.textContent = spec.note;
  }

  if (colorsRoot) {
    const picker = colorsRoot.querySelector("[data-look-colors-picker]");
    picker?.addEventListener("change", (event) => {
      pickedPalette = event.detail.index;
      syncColors();
    });
    syncColors();
  }

  const smoothRoot = document.querySelector("[data-look-smooth]");
  let pickedSmooth = 0;

  function reach(n) {
    return 1 - 2 ** (-1 / n);
  }

  function svgEl(tag, attrs = {}) {
    const node = document.createElementNS(SVG_NS, tag);
    Object.entries(attrs).forEach(([k, v]) => node.setAttribute(k, String(v)));
    return node;
  }

  function buildSmoothPath(n, ox, oy, r, endX, endY) {
    const box = r * (reach(2) / reach(n));
    const steps = 48;
    const pts = [`M ${endX.toFixed(2)} ${oy.toFixed(2)}`, `L ${(ox + box).toFixed(2)} ${oy.toFixed(2)}`];
    for (let i = 0; i <= steps; i++) {
      const t = (i / steps) * (Math.PI / 2);
      const x = ox + box * (1 - Math.sin(t) ** (2 / n));
      const y = oy + box * (1 - Math.cos(t) ** (2 / n));
      pts.push(`L ${x.toFixed(2)} ${y.toFixed(2)}`);
    }
    pts.push(`L ${ox.toFixed(2)} ${endY.toFixed(2)}`);
    return { d: pts.join(" "), box };
  }

  function renderSmoothDiagram() {
    if (!smoothRoot) return;
    const svg = smoothRoot.querySelector("[data-look-smooth-svg]");
    const note = smoothRoot.querySelector("[data-look-smooth-note]");
    if (!svg) return;
    const ox = 24;
    const oy = 24;
    const r = 64;
    const endX = 246;
    const endY = 176;
    const mid = r * reach(2);
    svg.replaceChildren();

    svg.append(
      svgEl("path", {
        d: `M ${endX} ${oy} H ${ox} V ${endY}`,
        class: "docs-lk-svg-frame",
      }),
      svgEl("line", {
        x1: ox,
        y1: oy,
        x2: ox + mid + 18,
        y2: oy + mid + 18,
        class: "docs-lk-svg-diag",
      }),
    );

    const activeN = pickedSmooth === 0 ? 0 : SMOOTH_LEVELS[pickedSmooth - 1];
    SMOOTH_LEVELS.forEach((n) => {
      const { d, box } = buildSmoothPath(n, ox, oy, r, endX, endY);
      const focused = activeN === 0 ? n === 5 : n === activeN;
      const muted = activeN !== 0 && n !== activeN;
      svg.append(
        svgEl("path", {
          d,
          class: "docs-lk-svg-curve",
          "data-n": String(n),
          "data-active": focused ? "1" : "0",
          "data-muted": muted ? "1" : "0",
        }),
      );
      if (activeN === 0 || n === activeN) {
        svg.append(
          svgEl("line", {
            x1: (ox + box).toFixed(2),
            y1: oy - 5,
            x2: (ox + box).toFixed(2),
            y2: oy + 5,
            class: "docs-lk-svg-tick",
            "data-active": focused ? "1" : "0",
          }),
          svgEl("text", {
            x: (ox + box).toFixed(2),
            y: oy - 9,
            class: "docs-lk-svg-label",
            "data-active": focused ? "1" : "0",
            "text-anchor": "middle",
          }),
        );
        svg.lastChild.textContent = String(n);
      }
    });

    svg.append(
      svgEl("circle", {
        cx: (ox + mid).toFixed(2),
        cy: (oy + mid).toFixed(2),
        r: "4",
        class: "docs-lk-svg-mid",
      }),
    );

    const tag = svgEl("text", {
      x: (ox + mid + 10).toFixed(2),
      y: (oy + mid + 4).toFixed(2),
      class: "docs-lk-svg-mid-label",
    });
    tag.textContent = "shared 45° midpoint";
    svg.append(tag);

    smoothRoot.querySelectorAll("[data-look-smooth-card]").forEach((card) => {
      const n = Number.parseInt(card.dataset.lookSmoothCard, 10);
      card.toggleAttribute("data-picked", activeN === n);
    });
    if (note) note.textContent = SMOOTH_NOTES[activeN] || SMOOTH_NOTES[0];
  }

  if (smoothRoot) {
    const seg = smoothRoot.querySelector("[data-look-smooth-seg]");
    seg?.addEventListener("change", (event) => {
      pickedSmooth = event.detail.index;
      renderSmoothDiagram();
    });
    smoothRoot.querySelectorAll("[data-look-smooth-card]").forEach((card) => {
      card.addEventListener("click", () => {
        const n = Number.parseInt(card.dataset.lookSmoothCard, 10);
        const index = SMOOTH_LEVELS.indexOf(n) + 1;
        pickedSmooth = pickedSmooth === index ? 0 : index;
        if (seg) window.pennaSegmented?.set?.(seg, pickedSmooth);
        renderSmoothDiagram();
      });
    });
    renderSmoothDiagram();
  }

  function settleAll() {
    requestAnimationFrame(() => {
      window.pennaKit?.roundAll(document);
      window.pennaSegmented?.wire(document);
    });
  }

  document.addEventListener("penna-theme", () => {
    syncTint();
    syncColors();
    settleAll();
  });

  settleAll();
})();
