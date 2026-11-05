const SURFACES = ".penna-button";

function shape(root = document) {
  root.querySelectorAll(SURFACES).forEach((button) => pennaKit.focusRing(button));
}

const FAR = 10000;

function layers(list) {
  const parts = [];
  let depth = 0;
  let start = 0;
  [...list].forEach((char, index) => {
    if (char === "(") depth += 1;
    if (char === ")") depth -= 1;
    if (char === "," && depth === 0) {
      parts.push(list.slice(start, index));
      start = index + 1;
    }
  });
  parts.push(list.slice(start));
  return parts.map((part) => part.trim());
}

function under(root = document) {
  root.querySelectorAll(`${SURFACES}[data-loading]`).forEach((button) => {
    button.style.removeProperty("--penna-under");
    button.style.removeProperty("--penna-over");
    const style = getComputedStyle(button);
    button.style.setProperty("--penna-under-inset", `${-(parseFloat(style.borderTopWidth) || 0)}px`);
    const list = style.boxShadow;
    const all = !list || list === "none" ? [] : layers(list);
    const inner = all.filter((layer) => /\binset\b/.test(layer));
    const outer = all
      .filter((layer) => !/\binset\b/.test(layer))
      .map((layer) => layer.replace(/(-?[\d.]+)px/, (_, x) => `${parseFloat(x) + FAR}px`));
    button.style.setProperty("--penna-under", outer.join(", ") || "none");
    button.style.setProperty("--penna-over", inner.join(", ") || "none");
  });
}

function fit() {
  pennaKit.ceilText();
  shape();
  under();
}

fit();
document.fonts.ready.then(fit);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && fit(), true);
