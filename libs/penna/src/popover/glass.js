(() => {
  const SURFACES = ".penna-toast[data-penna], .penna-popover-panel[data-penna]";

  function number(filter, name, fallback) {
    const match = filter.match(new RegExp(`${name}\\(([-\\d.]+)(%?)\\)`));
    if (!match) return fallback;
    return match[2] ? parseFloat(match[1]) / 100 : parseFloat(match[1]);
  }

  function translate(filter) {
    if (!filter || filter === "none" || !/brightness|contrast/.test(filter)) return null;
    const lift = number(filter, "brightness", 1) - 1;
    const contrast = number(filter, "contrast", 1);
    const scale = 1 - 2 * lift;
    if (scale <= 0) return null;
    const rest = filter.replace(/\s*(brightness|contrast)\([^)]*\)/g, "").trim();
    return `${rest} brightness(${(1 / scale).toFixed(4)}) contrast(${(contrast * scale).toFixed(4)})`.trim();
  }

  function glaze(root = document) {
    root.querySelectorAll(SURFACES).forEach((surface) => {
      surface.style.removeProperty("--penna-backdrop");
      const fixed = translate(getComputedStyle(surface).getPropertyValue("--penna-backdrop").trim());
      if (fixed) surface.style.setProperty("--penna-backdrop", fixed);
    });
  }

  glaze();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && glaze(), true);
  window.pennaGlass = { glaze };
})();
