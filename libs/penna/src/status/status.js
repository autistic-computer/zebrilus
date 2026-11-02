(() => {
  const TEXT = ".penna-text:not(.penna-chip-detail), .penna-button .penna-label";

  function snap(root = document) {
    const runs = [...root.querySelectorAll(TEXT)];
    runs.forEach((run) => run.style.removeProperty("width"));
    const widths = runs.map((run) => run.getBoundingClientRect().width);
    runs.forEach((run, index) => {
      run.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  const CORNER = 0.45;
  const FULL = 9999;

  function round(root = document) {
    root.querySelectorAll(".penna-chip").forEach((chip) => {
      const style = getComputedStyle(chip);
      const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
      if (radius <= 0 || radius >= FULL) {
        chip.removeAttribute("data-radius");
        return;
      }
      const edge = parseFloat(style.borderTopWidth) || 0;
      const cap = (Math.min(chip.offsetWidth, chip.offsetHeight) - 2 * edge) * CORNER;
      chip.style.setProperty("--penna-chip-r", `${cap * Math.tanh(radius / cap)}px`);
      chip.setAttribute("data-radius", "");
    });
  }

  const probe = new OffscreenCanvas(1, 1).getContext("2d");

  const channels = (color) => {
    probe.clearRect(0, 0, 1, 1);
    probe.fillStyle = color;
    probe.fillRect(0, 0, 1, 1);
    return [...probe.getImageData(0, 0, 1, 1).data.slice(0, 3)].map((value) => value / 255);
  };

  const linear = (value) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  const encoded = (value) => {
    const clamped = Math.min(Math.max(value, 0), 1);
    return clamped <= 0.0031308 ? clamped * 12.92 : 1.055 * clamped ** (1 / 2.4) - 0.055;
  };

  const amount = (filter, name, fallback) => {
    const found = filter.match(new RegExp(`${name}\\(([^)]*)\\)`));
    return found ? parseFloat(found[1]) : fallback;
  };

  function glass(root = document) {
    const ground = channels(getComputedStyle(document.documentElement).getPropertyValue("--penna-bg").trim());
    root.querySelectorAll(".penna-chip").forEach((chip) => {
      const filter = getComputedStyle(chip).getPropertyValue("--penna-backdrop").trim();
      if (!filter.startsWith("blur")) {
        chip.removeAttribute("data-glass");
        return;
      }
      const saturation = amount(filter, "saturate", 1);
      const lift = amount(filter, "brightness", 1) - 1;
      const contrast = amount(filter, "contrast", 1);
      const lit = ground.map(linear);
      const luminance = 0.2126 * lit[0] + 0.7152 * lit[1] + 0.0722 * lit[2];
      const under = lit.map((value) => {
        const toned = luminance + (value - luminance) * saturation;
        return Math.round(encoded((toned - 0.5) * contrast + 0.5 + lift) * 255);
      });
      chip.style.setProperty("--penna-glass-under", `rgb(${under.join(" ")})`);
      chip.setAttribute("data-glass", "");
    });
  }

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

  const STATES = ["", "hover", "active"];

  const fillOf = (tone, state) => {
    const probe = document.createElement("span");
    probe.hidden = true;
    probe.dataset.penna = "button";
    probe.toggleAttribute("data-prominent", true);
    if (tone) probe.dataset.tone = tone;
    if (state) probe.dataset.state = state;
    document.body.append(probe);
    const fill = getComputedStyle(probe).getPropertyValue("--penna-fill").trim();
    probe.remove();
    return channels(fill);
  };

  const toward = (ink, rest, moved) =>
    ink.map((value, index) => {
      const step = moved[index] - rest[index];
      const wash = step > 0 ? 1 : 0;
      const room = wash - rest[index];
      const share = room === 0 ? 0 : step / room;
      return Math.round((value + (wash - value) * share) * 255);
    });

  function ground(root = document) {
    const page = getComputedStyle(document.documentElement);
    const foreground = channels(page.getPropertyValue("--penna-primary").trim()).join() === channels(page.getPropertyValue("--penna-fg").trim()).join();
    root.querySelectorAll(".penna-notice .penna-button[data-prominent]").forEach((button) => {
      const carrier = button.closest(".penna-notice-tone");
      if (!carrier || !foreground) {
        button.removeAttribute("data-on");
        return;
      }
      const ink = channels(getComputedStyle(carrier).getPropertyValue("--penna-ink").trim());
      const [rest, ...moved] = STATES.map((state) => fillOf(button.dataset.tone, state));
      ["", "-hover", "-active"].forEach((suffix, index) => {
        const color = index === 0 ? ink.map((value) => Math.round(value * 255)) : toward(ink, rest, moved[index - 1]);
        button.style.setProperty(`--penna-on-fill${suffix}`, `rgb(${color.join(" ")})`);
      });
      button.setAttribute("data-on", "");
    });
  }

  function settle() {
    ground();
    snap();
    truncate();
    round();
    glass();
    ring();
  }

  settle();
  document.fonts.ready.then(settle);
  document.fonts.addEventListener("loadingdone", settle);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settle(), true);
  new ResizeObserver(() => truncate()).observe(document.body);
})();
