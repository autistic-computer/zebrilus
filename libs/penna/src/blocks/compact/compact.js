(() => {
  function groups() {
    document.querySelectorAll(".penna-compact-group").forEach((group) => {
      const face = group.querySelector(".penna-compact-group-face");
      const edge = parseFloat(getComputedStyle(face).borderLeftWidth) || 0;
      group.style.paddingInline = `${6 + edge}px`;
    });
  }

  function frames() {
    document.querySelectorAll(".penna-phone, .penna-key-slot").forEach((frame) => {
      const radius = parseFloat(getComputedStyle(frame).getPropertyValue("--penna-radius")) || 0;
      const cap = Math.min(frame.offsetWidth, frame.offsetHeight) * 0.45;
      const corner = cap > 0 ? cap * Math.tanh(radius / cap) : 0;
      const [box, k] = window.pennaKit?.drawnCorner(corner, frame.offsetWidth, frame.offsetHeight) ?? [corner, 1];
      frame.style.borderRadius = `${box.toFixed(2)}px`;
      frame.style.setProperty("corner-shape", `superellipse(${k.toFixed(3)})`);
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
      pill.style.removeProperty("--penna-fill");
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
      if (reads(alpha)) return;
      let [low, high] = [alpha, Math.max(0.95, alpha)];
      for (let i = 0; i < 12; i++) {
        const middle = (low + high) / 2;
        if (reads(middle)) high = middle;
        else low = middle;
      }
      alpha = high;
      pill.style.setProperty("--penna-fill", `rgb(${tint.r * 255} ${tint.g * 255} ${tint.b * 255} / ${alpha.toFixed(4)})`);
    });
  }

  function tones() {
    document.querySelectorAll('.penna-pill-tile[data-tone]').forEach((tile) => {
      const fill = rgba(getComputedStyle(tile).getPropertyValue("--penna-fill").trim() || "transparent");
      tile.toggleAttribute("data-bare", fill.a === 0);
    });
  }

  function presses() {
    document.querySelectorAll(".penna-bento-tile").forEach((tile) => {
      if (tile.querySelector(":scope > .penna-press")) return;
      const press = document.createElement("span");
      press.className = "penna-press";
      tile.append(press);
    });
  }

  presses();
  groups();
  tones();
  glassOver();
  frames();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && (groups(), frames(), tones(), glassOver()), true);
})();
