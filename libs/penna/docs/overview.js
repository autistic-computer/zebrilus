(() => {
  const root = document.querySelector("[data-docs-overview]");
  if (!root) return;

  const stage = root.querySelector("[data-overview-stage]");
  const floatWrap = root.querySelector("[data-overview-float-wrap]");
  const floatLabel = root.querySelector("[data-overview-float-label]");
  const floatSub = root.querySelector("[data-overview-float-sub]");
  const swatches = [...root.querySelectorAll("[data-overview-accent]")];
  const cornerControl = root.querySelector("[data-overview-corners]");
  const densityControl = root.querySelector("[data-overview-density]");
  const glassControl = root.querySelector("[data-overview-glass]");
  const listItems = [...root.querySelectorAll("[data-overview-row]")];

  const CORNERS = ["radius-4", "default", "radius-20", "pill"];
  const DENSITIES = ["24px", "28px", "32px"];
  const ACCENTS = {
    blue: { light: "#0a64dd", dark: "#3183ff" },
    purple: { light: "#8549d1", dark: "#a366f7" },
    pink: { light: "#c02a82", dark: "#e648a0" },
    red: { light: "#cc252d", dark: "#f44647" },
    orange: { light: "#ad4f00", dark: "#da6500" },
    green: { light: "#007b2c", dark: "#009c3a" },
    graphite: { light: "#6a6a6a", dark: "#868686" },
  };

  let pickedAccent = "blue";

  function mode() {
    return document.documentElement.dataset.pennaMode === "light" ? "light" : "dark";
  }

  function applyAccent() {
    const m = mode();
    swatches.forEach((btn) => {
      const name = btn.dataset.overviewAccent;
      const swatch = ACCENTS[name]?.[m];
      if (swatch) btn.style.setProperty("--overview-swatch", swatch);
      const selected = name === pickedAccent;
      btn.setAttribute("aria-pressed", String(selected));
      btn.closest(".docs-ov-ring")?.toggleAttribute("data-selected", selected);
    });
    if (!stage) return;
    const color = ACCENTS[pickedAccent]?.[m];
    if (color) {
      stage.style.setProperty("--penna-accent", color);
      stage.style.setProperty("--penna-accent-ink", color);
    }
  }

  function applyCorners(index) {
    if (!stage) return;
    stage.dataset.look = CORNERS[index] ?? "default";
    requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
  }

  function applyDensity(index) {
    if (!stage) return;
    stage.style.setProperty("--penna-control-md", DENSITIES[index] ?? "28px");
    requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
  }

  function applyGlass(on) {
    if (!floatWrap) return;
    floatWrap.dataset.look = on ? "glass-liquid" : "glass-none";
    if (floatLabel) floatLabel.textContent = on ? "Liquid glass" : "Solid surface";
    if (floatSub) floatSub.textContent = on ? "Blur, tint and perimeter rim" : "Solid popover fill and hairline";
    requestAnimationFrame(() => window.pennaKit?.roundAll?.(floatWrap));
  }

  swatches.forEach((btn) => {
    btn.addEventListener("click", () => {
      pickedAccent = btn.dataset.overviewAccent || "blue";
      applyAccent();
    });
  });

  cornerControl?.addEventListener("change", (event) => applyCorners(event.detail.index));
  densityControl?.addEventListener("change", (event) => applyDensity(event.detail.index));
  glassControl?.addEventListener("change", (event) => applyGlass(Boolean(event.detail.checked)));

  listItems.forEach((row) => {
    const pick = () => {
      listItems.forEach((other) => {
        const on = other === row;
        other.setAttribute("aria-selected", String(on));
        if (on) other.dataset.state = "selected";
        else delete other.dataset.state;
      });
    };
    row.addEventListener("click", pick);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        pick();
      }
    });
  });

  document.addEventListener("penna-theme", () => {
    applyAccent();
    if (stage) requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
  });

  applyAccent();
  applyGlass(true);
})();
