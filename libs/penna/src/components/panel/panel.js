const PANEL_FULL = 9999;

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
  panelInset();
  panelRules();
}

panelFit();
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && panelFit(), true);
