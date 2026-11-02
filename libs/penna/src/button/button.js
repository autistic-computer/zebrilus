const LABELS = ".penna-button .penna-label";
const SURFACES = ".penna-button";
const FULL = 9999;

function snap(root = document) {
  const labels = [...root.querySelectorAll(LABELS)];
  labels.forEach((label) => label.style.removeProperty("width"));
  const widths = labels.map((label) => label.getBoundingClientRect().width);
  labels.forEach((label, index) => {
    label.style.width = `${Math.ceil(widths[index])}px`;
  });
}

const RING = 1.5;
const CORNER = 0.45;

function shape(root = document) {
  root.querySelectorAll(SURFACES).forEach((button) => {
    const style = getComputedStyle(button);
    const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
    const full = radius >= FULL;
    const reach = RING + (parseFloat(style.borderTopWidth) || 0);
    const box = button.getBoundingClientRect();
    const shorter = Math.min(box.width, box.height) + 2 * RING;
    const cap = shorter * CORNER;
    const ring = full ? FULL : cap > 0 ? cap * Math.tanh((radius + RING) / cap) : 0;
    button.style.setProperty("--penna-ring-inset", `${-reach}px`);
    button.style.setProperty("--penna-ring-r", `${ring}px`);
  });
}

function fit() {
  snap();
  shape();
}

fit();
document.fonts.ready.then(fit);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && shape(), true);
