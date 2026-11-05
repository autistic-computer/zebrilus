const PANEL_FULL = 9999;

function panelInset() {
  document.querySelectorAll(".penna-panel[data-inset]").forEach((panel) => {
    const inset = Number(panel.dataset.inset);
    if (!Number.isFinite(inset)) return;
    panel.style.removeProperty("--penna-r");
    const radius = parseFloat(getComputedStyle(panel).getPropertyValue("--penna-r")) || 0;
    if (radius > 0 && radius < PANEL_FULL) {
      panel.style.setProperty("--penna-r", `${radius + inset}px`);
    }
  });
  window.pennaKit?.roundAll();
}

panelInset();
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && panelInset(), true);
