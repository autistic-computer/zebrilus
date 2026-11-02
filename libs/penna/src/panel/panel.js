const PANEL_TEXT = ".panel-text:not(.panel-wrap)";
const PANEL_FULL = 9999;

function panelSnap() {
  const spans = [...document.querySelectorAll(PANEL_TEXT)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function panelInset() {
  document.querySelectorAll("[data-inset]").forEach((panel) => {
    panel.style.removeProperty("--penna-r");
    const radius = parseFloat(getComputedStyle(panel).getPropertyValue("--penna-r")) || 0;
    if (radius > 0 && radius < PANEL_FULL) {
      panel.style.setProperty("--penna-r", `${radius + parseFloat(panel.dataset.inset)}px`);
    }
  });
  window.pennaKit?.roundAll();
}

function panelFilter(filter, name, fallback) {
  const match = filter.match(new RegExp(`${name}\\(([-\\d.]+)(%?)\\)`));
  if (!match) return fallback;
  return match[2] ? parseFloat(match[1]) / 100 : parseFloat(match[1]);
}

function panelBackdrop(filter) {
  if (!/brightness|contrast/.test(filter)) return null;
  const lift = panelFilter(filter, "brightness", 1) - 1;
  const scale = 1 - 2 * lift;
  if (scale <= 0) return null;
  const contrast = panelFilter(filter, "contrast", 1);
  const rest = filter.replace(/\s*(brightness|contrast)\([^)]*\)/g, "").trim();
  return `${rest} brightness(${(1 / scale).toFixed(4)}) contrast(${(contrast * scale).toFixed(4)})`.trim();
}

function panelGlass() {
  document.querySelectorAll(".penna-panel[data-penna]").forEach((panel) => {
    panel.style.removeProperty("--penna-backdrop");
    const backdrop = getComputedStyle(panel).getPropertyValue("--penna-backdrop").trim();
    if (backdrop === "" || backdrop === "none") return;
    const fixed = panelBackdrop(backdrop);
    if (fixed) panel.style.setProperty("--penna-backdrop", fixed);
  });
}

function panelRules() {
  document.querySelectorAll("[data-rule]").forEach((element) => {
    const probe = document.createElement("i");
    probe.dataset.penna = element.dataset.rule;
    probe.hidden = true;
    document.body.append(probe);
    const width = parseFloat(getComputedStyle(probe).getPropertyValue("--penna-bw")) || 0;
    probe.remove();
    element.toggleAttribute("data-ruled", width > 0);
  });
}

function panelFit() {
  panelSnap();
  panelInset();
  panelGlass();
  panelRules();
}

panelFit();
document.fonts.ready.then(panelFit);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && (panelInset(), panelGlass(), panelRules()), true);
