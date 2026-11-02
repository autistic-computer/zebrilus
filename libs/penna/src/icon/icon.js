const GAMMA = [0.14806, -0.89458, 1.47596, -0.32467];
const CONTRAST = 1;
const SAMPLES = Array.from({ length: 19 }, (_, index) => (index + 1) / 20);
const STEPS = Array.from({ length: 51 }, (_, index) => index / 50);

function channels(color) {
  return color.match(/[\d.]+/g)?.slice(0, 3).map(Number) ?? [0, 0, 0];
}

function brightness(color) {
  const [r, g, b] = channels(color).map((value) => value / 255);
  return 0.3 * r + 0.59 * g + 0.11 * b;
}

function corrected(alpha, light) {
  const contrast = CONTRAST * Math.min(Math.max(4 * (0.75 - light), 0), 1);
  const a = (alpha * (contrast + 1)) / (alpha * contrast + 1);
  const correction = (GAMMA[0] * light + GAMMA[1]) * a + (GAMMA[2] * light + GAMMA[3]);
  return a + a * (1 - a) * correction;
}

const over = (a, k) => a + k * a * (1 - a);

function shadows(color) {
  const light = brightness(color);
  const target = SAMPLES.map((a) => corrected(a, light));
  let best = { error: Infinity, chain: [] };
  STEPS.forEach((first) => {
    STEPS.forEach((second) => {
      let error = 0;
      SAMPLES.forEach((a, index) => {
        error += (over(over(a, first), second) - target[index]) ** 2;
      });
      if (error < best.error) best = { error, chain: [first, second] };
    });
  });
  const [r, g, b] = channels(color);
  const layers = best.chain.filter((k) => k > 0).map((k) => `drop-shadow(0 0 0 rgb(${r} ${g} ${b} / ${k}))`);
  return layers.length ? layers.join(" ") : "none";
}

const cache = new Map();

function tune(root = document) {
  root.querySelectorAll(".penna-glyph").forEach((glyph) => {
    const icon = glyph.querySelector("[data-icon]");
    if (!icon) return;
    const color = getComputedStyle(icon).color;
    if (!cache.has(color)) cache.set(color, shadows(color));
    glyph.style.filter = cache.get(color);
  });
}

tune();
window.addEventListener("load", () => tune());
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(() => tune()), true);
