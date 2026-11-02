(() => {
  const TEXT = "section[data-section] .penna-text";

  function snapText() {
    const spans = [...document.querySelectorAll(TEXT)];
    spans.forEach((span) => span.style.removeProperty("width"));
    const widths = spans.map((span) => span.getBoundingClientRect().width);
    spans.forEach((span, index) => {
      span.style.display = "inline-block";
      span.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  function wireMenus() {
    document.querySelectorAll(".penna-pill-menu-page").forEach((page) => {
      const menu = page.querySelector(".penna-pill-menu");
      const shut = () => menu.removeAttribute("data-open");
      menu.querySelector(".penna-pill-menu-scrim").addEventListener("click", shut);
      menu.querySelector(".penna-pill-menu-close").addEventListener("click", shut);
      menu.querySelectorAll(".penna-pill-chip").forEach((chip) => chip.addEventListener("click", shut));
      page.addEventListener("click", (event) => {
        if (!menu.hasAttribute("data-open") && !menu.contains(event.target)) menu.setAttribute("data-open", "");
      });
    });
  }

  function tone(backdrop) {
    const value = (name, fallback) => {
      const match = backdrop.match(new RegExp(`${name}\\(([-\\d.]+)\\)`));
      return match ? parseFloat(match[1]) : fallback;
    };
    const lift = value("brightness", 1) - 1;
    const contrast = value("contrast", 1);
    if (!lift && contrast === 1) return backdrop;
    const offset = 0.5 * (1 - contrast) + lift;
    const after = 1 - 2 * offset;
    const scale = contrast / after;
    return backdrop
      .replace(/\s*brightness\([^)]*\)/, "")
      .replace(/\s*contrast\([^)]*\)/, "")
      .concat(` brightness(${scale.toFixed(4)}) contrast(${after.toFixed(4)})`);
  }

  function glass() {
    document.querySelectorAll("section[data-section] [data-penna]").forEach((element) => {
      element.style.removeProperty("backdrop-filter");
      if (getComputedStyle(element).backdropFilter === "none") return;
      const backdrop = getComputedStyle(element).getPropertyValue("--penna-backdrop").trim();
      if (!backdrop || backdrop === "none") return;
      element.style.backdropFilter = tone(backdrop);
    });
  }

  function groups() {
    document.querySelectorAll(".penna-compact-group").forEach((group) => {
      const face = group.querySelector(".penna-compact-group-face");
      const edge = parseFloat(getComputedStyle(face).borderLeftWidth) || 0;
      group.style.paddingInline = `${6 + edge}px`;
    });
  }

  function smoothing() {
    const probe = document.createElement("div");
    probe.dataset.penna = "panel";
    probe.hidden = true;
    document.body.append(probe);
    const k = getComputedStyle(probe).getPropertyValue("--penna-style-k").trim() || "1";
    probe.remove();
    return k;
  }

  function frames() {
    const k = smoothing();
    document.querySelectorAll(".penna-phone, .penna-key-slot").forEach((frame) => {
      frame.style.cornerShape = `superellipse(${k})`;
      const radius = parseFloat(getComputedStyle(frame).getPropertyValue("--penna-radius")) || 0;
      const cap = Math.min(frame.offsetWidth, frame.offsetHeight) * 0.45;
      frame.style.borderRadius = cap > 0 ? `${(cap * Math.tanh(radius / cap)).toFixed(2)}px` : "0px";
    });
  }

  function rgba(color) {
    const probe = document.createElement("canvas").getContext("2d");
    probe.fillStyle = color;
    probe.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = probe.getImageData(0, 0, 1, 1).data;
    return { r: r / 255, g: g / 255, b: b / 255, a: a / 255 };
  }

  function luminance({ r, g, b }) {
    const linear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
  }

  function contrast(left, right) {
    const [a, b] = [luminance(left), luminance(right)];
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  }

  function glassOver() {
    document.querySelectorAll('.penna-phone[data-page] .penna-compact-pill').forEach((pill) => {
      pill.style.removeProperty("background");
      const style = getComputedStyle(pill);
      const backdrop = style.getPropertyValue("--penna-backdrop").trim();
      if (!backdrop || backdrop === "none") return;
      const page = rgba(getComputedStyle(pill.closest(".penna-phone")).backgroundColor);
      const tint = rgba(style.getPropertyValue("--penna-fill"));
      const ink = rgba(style.getPropertyValue("--penna-ink"));
      const shows = (alpha) => ({
        r: page.r + (tint.r - page.r) * alpha,
        g: page.g + (tint.g - page.g) * alpha,
        b: page.b + (tint.b - page.b) * alpha,
      });
      const reads = (alpha) => contrast(ink, shows(alpha)) >= 4.5;
      let alpha = tint.a;
      if (!reads(alpha)) {
        let [low, high] = [alpha, Math.max(0.95, alpha)];
        for (let i = 0; i < 12; i++) {
          const middle = (low + high) / 2;
          if (reads(middle)) high = middle;
          else low = middle;
        }
        alpha = high;
      }
      const color = shows(alpha);
      pill.style.background = `rgb(${color.r * 255} ${color.g * 255} ${color.b * 255})`;
    });
  }

  function tones() {
    document.querySelectorAll('.penna-pill-tile[data-tone]').forEach((tile) => {
      const fill = rgba(getComputedStyle(tile).getPropertyValue("--penna-fill").trim() || "transparent");
      tile.toggleAttribute("data-bare", fill.a === 0);
    });
  }

  function prominent() {
    const probe = document.createElement("div");
    probe.dataset.penna = "phone.pill";
    probe.hidden = true;
    document.body.append(probe);
    const style = getComputedStyle(probe);
    const backdrop = style.getPropertyValue("--penna-backdrop").trim();
    const alpha = rgba(style.getPropertyValue("--penna-fill").trim() || "transparent").a;
    probe.remove();
    const glassy = Boolean(backdrop) && backdrop !== "none";
    document.querySelectorAll('.penna-pill-menu-close, .penna-pill-chip[data-look="primary"]').forEach((element) => {
      element.toggleAttribute("data-glass", glassy);
      element.style.setProperty("--penna-pill-alpha", alpha.toFixed(3));
    });
  }

  function presses() {
    document.querySelectorAll(".penna-bento-tile").forEach((tile) => {
      if (tile.querySelector(":scope > .penna-press")) return;
      const press = document.createElement("span");
      press.className = "penna-press";
      press.dataset.penna = "button.quiet";
      press.dataset.state = "active";
      tile.append(press);
    });
  }

  snapText();
  presses();
  prominent();
  groups();
  tones();
  glassOver();
  frames();
  glass();
  wireMenus();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && (prominent(), groups(), frames(), glass(), tones(), glassOver()), true);
  document.fonts.ready.then(snapText);
})();
