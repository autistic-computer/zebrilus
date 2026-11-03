(() => {
  const { Menu, dropdown, context } = globalThis.pennaMenu;

  const stepper = (label, initial, step, format) => ({ kind: "stepper", label, value: initial, initial, step, format });

  const menus = {
    pane: () => [
      { kind: "label", label: "Pane" },
      { label: "New tab", icon: "plus", key: "cmd-t" },
      { label: "Split right", icon: "panel-right", key: "cmd-d" },
      { label: "Split down", icon: "panel-bottom", key: "shift-cmd-d" },
      { kind: "separator" },
      { label: "Synchronize input", checked: true },
      { label: "Monitor activity" },
      {
        kind: "submenu",
        label: "Move to",
        icon: "layers",
        items: [
          { label: "work" },
          { label: "scratch", checked: true },
          { label: "logs" },
          { kind: "separator" },
          { label: "New session", icon: "plus" },
        ],
      },
      { kind: "separator" },
      { label: "Copy path", icon: "copy", key: "alt-cmd-c" },
      { label: "Rename", disabled: true },
      { label: "Close pane", icon: "xmark", key: "cmd-w" },
    ],
    view: () => [
      { kind: "label", label: "View" },
      stepper("Zoom", 100, 0, (value) => `${value}%`),
      stepper("Font size", 13, 0, (value) => `${value}`),
      { kind: "separator" },
      { label: "Light", icon: "sun" },
      { label: "Dark", icon: "moon", checked: true },
      { kind: "separator" },
      { label: "Documentation", link: "https://example.com" },
      { label: "Changelog", link: "https://example.com", disabled: true },
      { kind: "note", label: "Version 0.16.0", disabled: true },
    ],
    layout: () => [
      { label: "Even horizontal", checked: true },
      { label: "Even vertical" },
      { label: "Main vertical" },
      { label: "Tiled" },
      { kind: "separator" },
      { label: "Split right", icon: "panel-right", key: "cmd-d" },
    ],
    region: () => [
      { label: "Copy", icon: "copy", key: "alt-cmd-c" },
      { label: "Paste" },
      { kind: "separator" },
      { label: "Detach" },
      { label: "Close pane", icon: "xmark", key: "cmd-w" },
    ],
  };

  let focus = null;
  document.querySelectorAll("[data-menus]").forEach((row) => {
    const built = row.dataset.menus.split(" ").map((name) => new Menu(menus[name]()));
    row.append(...built.map((menu) => menu.element));
    const [menu] = built;
    if (row.dataset.highlight === "row") {
      menu.select(2);
      focus = menu;
    }
    if (row.dataset.highlight === "submenu") {
      menu.select(7);
      menu.active().select(0);
    }
  });

  document.querySelectorAll("[data-dropdown]").forEach((trigger) => {
    const handle = dropdown(trigger, menus[trigger.dataset.dropdown]());
    if (trigger.hasAttribute("data-open")) handle.show();
  });

  document.querySelectorAll("[data-context]").forEach((region) => {
    const handle = context(region, menus[region.dataset.context]());
    if (region.hasAttribute("data-open")) handle.show(region.clientWidth / 2, region.clientHeight / 2);
  });

  const SVG = "http://www.w3.org/2000/svg";

  function outline(width, height, radius, exponent, stroke) {
    const speed = 1 / (3 * stroke);
    const arc = (radius * Math.PI) / 2;
    const across = width - 2 * radius;
    const down = height - 2 * radius;
    const reach = (phi) => radius / (Math.abs(Math.cos(phi)) ** exponent + Math.abs(Math.sin(phi)) ** exponent) ** (1 / exponent) - stroke / 2;
    const corner = (cx, cy, sx, sy, from, to) => (u) => {
      const phi = from + (to - from) * u;
      const rho = reach(phi);
      return [cx + sx * rho * Math.cos(phi), cy + sy * rho * Math.sin(phi)];
    };
    const half = stroke / 2;
    const pieces = [
      [across, (u) => [radius + across * u, half]],
      [arc, corner(width - radius, radius, 1, -1, Math.PI / 2, 0)],
      [down, (u) => [width - half, radius + down * u]],
      [arc, corner(width - radius, height - radius, 1, 1, 0, Math.PI / 2)],
      [across, (u) => [width - radius - across * u, height - half]],
      [arc, corner(radius, height - radius, -1, 1, Math.PI / 2, 0)],
      [down, (u) => [half, height - radius - down * u]],
      [arc, corner(radius, radius, -1, -1, 0, Math.PI / 2)],
    ].map(([length, at]) => ({ span: Math.max(0, length) * speed, at }));
    const total = pieces.reduce((sum, piece) => sum + piece.span, 0);
    const point = (t) => {
      for (const piece of pieces) {
        if (t <= piece.span || piece === pieces.at(-1)) return piece.at(piece.span ? Math.min(1, t / piece.span) : 0);
        t -= piece.span;
      }
    };
    const count = Math.floor(total);
    if (count < 1) return "";
    const period = total / count;
    const dash = 2 / 3;
    let d = "";
    for (let k = 0; k < count; k++) {
      const start = k * period;
      const steps = Math.max(2, Math.ceil(dash / 0.04));
      for (let i = 0; i <= steps; i++) {
        const [x, y] = point(start + (dash * i) / steps);
        d += `${i ? "L" : "M"}${x.toFixed(3)} ${y.toFixed(3)}`;
      }
    }
    return d;
  }

  function regions() {
    const k = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-window-k")) || 1;
    document.querySelectorAll(".menus-region").forEach((region) => {
      const radius = parseFloat(getComputedStyle(region).getPropertyValue("--penna-radius")) || 0;
      const width = region.offsetWidth;
      const height = region.offsetHeight;
      const cap = Math.min(width, height) * 0.45;
      const resolved = cap > 0 ? cap * Math.tanh(radius / cap) : 0;
      region.style.borderRadius = `${resolved}px`;
      let svg = region.querySelector(":scope > .menus-dashes");
      if (!svg) {
        svg = document.createElementNS(SVG, "svg");
        svg.classList.add("menus-dashes");
        svg.setAttribute("aria-hidden", "true");
        svg.append(document.createElementNS(SVG, "path"));
        region.prepend(svg);
      }
      svg.setAttribute("width", width);
      svg.setAttribute("height", height);
      svg.firstChild.setAttribute("d", outline(width, height, resolved, 2 ** k, 1));
    });
  }

  regions();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && regions(), true);
  focus?.focus();
})();
