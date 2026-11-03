(() => {
  const probe = new OffscreenCanvas(1, 1).getContext("2d");

  const channels = (color) => {
    probe.clearRect(0, 0, 1, 1);
    probe.fillStyle = color;
    probe.fillRect(0, 0, 1, 1);
    return [...probe.getImageData(0, 0, 1, 1).data.slice(0, 3)].map((value) => value / 255);
  };

  function ring(root = document) {
    const accent = channels(getComputedStyle(document.documentElement).getPropertyValue("--penna-accent").trim()).join();
    root.querySelectorAll(".penna-chip[data-ring]").forEach((chip) => {
      chip.style.removeProperty("--penna-shadow");
      const shadow = getComputedStyle(chip).getPropertyValue("--penna-shadow").trim();
      const recolored = shadow.replace(/#[0-9a-f]{3,8}\b|rgba?\([^)]*\)/gi, (color) =>
        channels(color).join() === accent ? `var(--penna-${chip.dataset.ring})` : color,
      );
      chip.style.setProperty("--penna-shadow", recolored);
    });
  }

  const range = document.createRange();

  const inked = (element) => {
    range.selectNodeContents(element);
    return range.getBoundingClientRect().width;
  };

  function truncate(root = document) {
    root.querySelectorAll(".penna-chip-detail").forEach((detail) => {
      detail.dataset.text ??= detail.textContent;
      const text = detail.dataset.text;
      detail.textContent = text;
      detail.style.removeProperty("width");
      detail.style.width = `${Math.ceil(inked(detail))}px`;
      const room = detail.getBoundingClientRect().width;
      if (inked(detail) <= room) return;
      let low = 0;
      let high = text.length;
      while (low < high) {
        const middle = Math.ceil((low + high) / 2);
        detail.textContent = `${text.slice(0, middle).trimEnd()}…`;
        if (inked(detail) <= room) low = middle;
        else high = middle - 1;
      }
      detail.textContent = `${text.slice(0, low).trimEnd()}…`;
    });
  }

  function focus(root = document) {
    root.querySelectorAll('.penna-chip[data-state~="focused"]').forEach((chip) => {
      const style = getComputedStyle(chip);
      const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
      const box = chip.getBoundingClientRect();
      const cap = (Math.min(box.width, box.height) + 3) * 0.45;
      chip.style.setProperty("--penna-ring-inset", `${-(1.5 + (parseFloat(style.borderTopWidth) || 0))}px`);
      chip.style.setProperty("--penna-ring-r", `${cap > 0 ? cap * Math.tanh((radius + 1.5) / cap) : 0}px`);
    });
  }

  function settle() {
    focus();
    truncate();
    ring();
  }

  settle();
  document.fonts.ready.then(settle);
  document.fonts.addEventListener("loadingdone", settle);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settle(), true);
  new ResizeObserver(() => truncate()).observe(document.body);
})();
