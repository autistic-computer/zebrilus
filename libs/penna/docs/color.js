(() => {
  const root = document.querySelector("[data-docs-color]");
  if (!root) return;

  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });

  const ACCENTS = {
    blue: { label: "Blue", light: "#0a64dd", dark: "#3183ff" },
    purple: { label: "Purple", light: "#8549d1", dark: "#a366f7" },
    pink: { label: "Pink", light: "#c02a82", dark: "#e648a0" },
    red: { label: "Red", light: "#cc252d", dark: "#f44647" },
    orange: { label: "Orange", light: "#ad4f00", dark: "#da6500" },
    green: { label: "Green", light: "#007b2c", dark: "#009c3a" },
    graphite: { label: "Graphite", light: "#6a6a6a", dark: "#868686" },
  };

  let pickedAccent = "blue";

  function mode() {
    return document.documentElement.dataset.pennaMode === "light" ? "light" : "dark";
  }

  function rgba(color) {
    if (!ctx || !color) return [0, 0, 0, 1];
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "rgba(0,0,0,0)";
    ctx.fillStyle = color;
    ctx.fillRect(0, 0, 1, 1);
    const data = ctx.getImageData(0, 0, 1, 1).data;
    return [data[0], data[1], data[2], data[3] / 255];
  }

  function over(fg, bg) {
    const a = fg[3];
    return [
      Math.round(fg[0] * a + bg[0] * (1 - a)),
      Math.round(fg[1] * a + bg[1] * (1 - a)),
      Math.round(fg[2] * a + bg[2] * (1 - a)),
      1,
    ];
  }

  function hex(rgb) {
    return (
      "#" +
      rgb
        .slice(0, 3)
        .map((n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0"))
        .join("")
    );
  }

  function luminance(rgb) {
    const channel = (v) => {
      const c = v / 255;
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
  }

  function contrast(a, b) {
    const l1 = luminance(a);
    const l2 = luminance(b);
    return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
  }

  function prop(el, name) {
    return getComputedStyle(el).getPropertyValue(name).trim();
  }

  function resolveToken(el, token, fallbackBg) {
    const raw = token.startsWith("--") ? prop(el, token) : token;
    const c = rgba(raw);
    if (c[3] < 0.999 && fallbackBg) return over(c, fallbackBg);
    return c;
  }

  function refreshTokens() {
    const bg = resolveToken(document.documentElement, "--penna-bg");

    document.querySelectorAll("[data-color-hex]").forEach((el) => {
      const token = el.dataset.colorHex;
      const source = el.closest("[data-color-source]") || document.documentElement;
      const c = resolveToken(source, token, bg);
      el.textContent = hex(c);
    });

    document.querySelectorAll("[data-color-ratio]").forEach((el) => {
      const fgToken = el.dataset.colorRatio;
      const bgToken = el.dataset.on || "--penna-bg";
      const source = el.closest("[data-color-source]") || document.documentElement;
      const ground = resolveToken(source, bgToken, bg);
      const ink = resolveToken(source, fgToken, ground);
      const ratio = contrast(ink, ground);
      const grade = ratio >= 7 ? "AAA" : ratio >= 4.5 ? "AA" : "< 4.5";
      el.textContent = el.hasAttribute("data-ratio-bare")
        ? `${ratio.toFixed(1)}:1`
        : `${ratio.toFixed(1)}:1 · ${grade}`;
    });

    applyAccent();
  }

  const accentStage = document.querySelector("[data-color-accent-stage]");
  const accentSwatches = [...document.querySelectorAll("[data-color-accent]")];
  const accentNameEl = document.querySelector("[data-color-accent-name]");
  const accentHexEl = document.querySelector("[data-color-accent-hex]");
  const accentRatioEl = document.querySelector("[data-color-accent-ratio]");

  function applyAccent() {
    const m = mode();
    const bg = resolveToken(document.documentElement, "--penna-bg");
    accentSwatches.forEach((btn) => {
      const key = btn.dataset.colorAccent;
      const swatch = ACCENTS[key]?.[m];
      if (swatch) btn.style.setProperty("--overview-swatch", swatch);
      const selected = key === pickedAccent;
      btn.setAttribute("aria-pressed", String(selected));
      btn.closest(".docs-ov-ring")?.toggleAttribute("data-selected", selected);
    });
    const info = ACCENTS[pickedAccent];
    const color = info?.[m];
    if (!color) return;
    if (accentStage) {
      accentStage.style.setProperty("--penna-accent", color);
      accentStage.style.setProperty("--penna-accent-ink", color);
    }
    const c = rgba(color);
    const ratio = contrast(c, bg);
    if (accentNameEl) accentNameEl.textContent = info.label;
    if (accentHexEl) accentHexEl.textContent = color;
    if (accentRatioEl) accentRatioEl.textContent = `${ratio.toFixed(1)}:1 on bg`;
  }

  accentSwatches.forEach((btn) => {
    btn.addEventListener("click", () => {
      pickedAccent = btn.dataset.colorAccent || "blue";
      applyAccent();
    });
  });

  document.querySelectorAll("[data-copy-token]").forEach((btn) => {
    let timer = null;
    const original = btn.textContent;
    btn.addEventListener("click", () => {
      const value = btn.dataset.copyToken;
      if (!value) return;
      navigator.clipboard?.writeText(value).catch(() => {});
      clearTimeout(timer);
      btn.dataset.copied = "";
      btn.textContent = "Copied";
      timer = setTimeout(() => {
        delete btn.dataset.copied;
        btn.textContent = original;
      }, 1200);
    });
  });

  const schedule = () => requestAnimationFrame(refreshTokens);
  schedule();
  document.addEventListener("penna-theme", schedule);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && schedule(), true);
})();
