const script = document.currentScript;
const listed = script.dataset.themes?.split(/[\s,]+/).filter(Boolean) ?? [];
const THEMES = listed.length ? listed : ["default", "macos", "yaru"];
const MODES = ["dark", "light"];
const KEY = "penna-kit-theme";

const folder = script.dataset.tokens?.replace(/\/?$/, "/");
const tokens = new URL(folder || "tokens/", folder ? document.baseURI : script.src);
const params = new URLSearchParams(location.search);

function stored() {
  try {
    return JSON.parse(localStorage.getItem(KEY) || "{}") ?? {};
  } catch {
    return {};
  }
}

function remember(choice) {
  try {
    localStorage.setItem(KEY, JSON.stringify(choice));
  } catch {}
}

function themeName(value) {
  return typeof value === "string" && /^[a-z0-9][a-z0-9-]*$/.test(value) ? value : undefined;
}

function modeName(value) {
  return MODES.includes(value) ? value : undefined;
}

const showSwitcher = script.dataset.switcher !== "off" && !params.has("bare");
const saved = stored();
const choice = {
  theme: themeName(params.get("theme")) || (!params.has("bare") ? themeName(saved.theme) : undefined) || script.dataset.theme || THEMES[0],
  mode: modeName(params.get("mode")) || modeName(saved.mode) || "dark",
};

const density = params.get("density") || (!params.has("bare") ? saved.density : undefined);
if (density === "compact" || density === "comfortable") document.documentElement.dataset.density = density;
const glass = params.get("glass") || (!params.has("bare") ? saved.glass : undefined);
if (glass === "none") document.documentElement.dataset.glass = "none";
const motion = params.get("motion") || (!params.has("bare") ? saved.motion : undefined);
if (motion === "off" || motion === "0") document.documentElement.dataset.motion = "off";
if (params.get("pointer") === "arrow") document.documentElement.dataset.pointer = "arrow";

const swapping = new Map();
const active = new Map();

document.addEventListener(
  "load",
  (event) => {
    const job = swapping.get(event.target);
    if (!job) return;
    swapping.delete(event.target);
    if (active.get(job.id) === event.target) active.delete(job.id);
    job.existing?.remove();
    job.onReady?.();
    requestAnimationFrame(() => {
      roundAll();
      ceilTextRuns();
    });
  },
  true,
);

function load(id, file, onReady) {
  const existing = document.getElementById(id);
  const href = new URL(`${file}.css`, tokens).href;
  const inflight = active.get(id);
  if (inflight && inflight.href === href) return;
  if (inflight) {
    const prior = swapping.get(inflight);
    swapping.delete(inflight);
    active.delete(id);
    inflight.remove();
    if (prior?.existing && !document.getElementById(id)) prior.existing.id = id;
  }
  const current = document.getElementById(id);
  if (current && current.href === href) {
    onReady?.();
    return;
  }
  const link = document.createElement("link");
  link.id = id;
  link.rel = "stylesheet";
  link.href = href;
  if (!current) {
    swapping.set(link, { id, existing: null, onReady });
    active.set(id, link);
    document.head.prepend(link);
    return;
  }
  current.removeAttribute("id");
  swapping.set(link, { id, existing: current, onReady });
  active.set(id, link);
  link.addEventListener(
    "error",
    () => {
      swapping.delete(link);
      if (active.get(id) === link) active.delete(id);
      link.remove();
      if (!document.getElementById(id)) current.id = id;
    },
    { once: true },
  );
  current.after(link);
}

function ceilTextRuns(root = document, selector = "[data-penna-text]") {
  const runs = [...(root.matches?.(selector) ? [root] : []), ...root.querySelectorAll(selector)];
  runs.forEach((run) => run.style.removeProperty("width"));
  const widths = runs.map((run) => run.getBoundingClientRect().width);
  runs.forEach((run, index) => {
    if (widths[index] > 0) run.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function apply() {
  const initial = !document.getElementById("penna-tokens");
  const commit = () => {
    document.documentElement.dataset.pennaTheme = choice.theme;
    document.documentElement.dataset.pennaMode = choice.mode;
    if (!params.has("bare")) remember({ ...stored(), ...choice });
    document.dispatchEvent(new CustomEvent("penna-theme", { detail: { ...choice } }));
  };
  if (initial) commit();
  load("penna-icons", `${choice.theme}-icons`);
  load("penna-tokens", `${choice.theme}-${choice.mode}`, initial ? undefined : commit);
}

function apparentCorner(radius, half, k) {
  const exponent = 2 ** k;
  if (exponent <= 2 || radius <= 0 || half <= 0) return [radius, k];
  const reach = (n) => 1 - 2 ** (-1 / n);
  const circle = reach(2);
  const limit = half - 0.05;
  let grow = circle / reach(exponent);
  let eased = exponent;
  if (radius * grow > limit) {
    grow = Math.max(limit / radius, 1);
    eased = -1 / Math.log2(1 - circle / grow);
  }
  return [Math.min(radius * grow, half), Math.log2(eased)];
}

function cornerOf(style, width, height) {
  const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
  if (radius <= 0 || radius >= 9999) return radius;
  const share = parseFloat(style.getPropertyValue("--penna-share")) || 0;
  const shorter = Math.min(width, height);
  let corner;
  if (share > 0) {
    corner = Math.min(share, 0.5) * shorter;
  } else {
    const cap = shorter * 0.45;
    corner = cap > 0 ? cap * Math.tanh(radius / cap) : 0;
  }
  const around = style.getPropertyValue("--penna-around").trim();
  if (around) {
    const rows = parseFloat(style.getPropertyValue("--penna-around-rows")) || 28;
    const share = parseFloat(style.getPropertyValue("--penna-control-share")) || 0;
    const inset = parseFloat(around) || 0;
    corner = Math.min(share * rows + inset, rows / 2 + inset);
  }
  const inside = style.getPropertyValue("--penna-inside").trim();
  if (inside) {
    const rows = parseFloat(style.getPropertyValue("--penna-inside-rows")) || 28;
    const share = parseFloat(style.getPropertyValue("--penna-control-share")) || 0;
    corner = Math.max(Math.min(share * rows, rows / 2) - (parseFloat(inside) || 0), 0);
  }
  const nest = style.getPropertyValue("--penna-nest-inset").trim();
  if (nest) {
    const outer = parseFloat(style.getPropertyValue("--penna-nest-outer")) || parseFloat(style.getPropertyValue("--penna-radius")) || 0;
    corner = Math.max(Math.min(corner, outer - (parseFloat(nest) || 0)), 0);
  }
  return corner;
}

function focusRing(element, style = getComputedStyle(element)) {
  const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
  const share = parseFloat(style.getPropertyValue("--penna-share")) || 0;
  const box = element.getBoundingClientRect();
  const reach = 1.5 + (parseFloat(style.getPropertyValue("--penna-ring-gap")) || 0);
  const shorter = Math.min(box.width, box.height) + 2 * reach;
  const cap = shorter * 0.45;
  let ring;
  if (radius >= 9999) ring = 9999;
  else if (radius <= 0) ring = 0;
  else if (share > 0) ring = Math.min(share, 0.5) * shorter;
  else ring = cap > 0 ? cap * Math.tanh((radius + reach) / cap) : 0;
  const k = ring >= 9999 ? 1 : parseFloat(style.getPropertyValue("--penna-k")) || 1;
  const [ringBox, ringShape] = ring >= 9999 ? [ring, 1] : apparentCorner(ring, shorter / 2, k);
  element.style.setProperty("--penna-ring-inset", `${-(reach + (parseFloat(style.borderTopWidth) || 0))}px`);
  element.style.setProperty("--penna-ring-r", `${ringBox}px`);
  element.style.setProperty("--penna-ring-k", `${ringShape}`);
}

function clipParent(element, style) {
  const clip = element.parentElement;
  if (!clip?.hasAttribute("data-penna-clip") || clip.firstElementChild !== element) return;
  clip.style.borderRadius = element.style.borderRadius || style.borderTopLeftRadius;
  clip.style.setProperty("corner-shape", element.style.getPropertyValue("corner-shape") || style.getPropertyValue("corner-shape"));
}

function round(element) {
  const style = getComputedStyle(element);
  focusRing(element, style);
  const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
  if (radius <= 0 || radius >= 9999) {
    element.style.removeProperty("border-radius");
    element.style.removeProperty("corner-shape");
    element.style.removeProperty("--penna-corner");
    clipParent(element, style);
    return;
  }
  const width = element.offsetWidth;
  const height = element.offsetHeight;
  if (!width || !height) return;
  const corner = cornerOf(style, width, height);
  const k = parseFloat(style.getPropertyValue("--penna-k")) || 1;
  const [box, shape] = apparentCorner(corner, Math.min(width, height) / 2, k);
  element.style.borderRadius = `${box.toFixed(2)}px`;
  element.style.setProperty("corner-shape", `superellipse(${shape.toFixed(3)})`);
  element.style.setProperty("--penna-corner", `${corner.toFixed(2)}px`);
  clipParent(element, style);
}

const SVG = "http://www.w3.org/2000/svg";
const refracts = Boolean(navigator.userAgentData) && !params.has("flat");
const lenses = new WeakMap();
let lensCount = 0;
let lensRoot;

function lensMap(width, height, radius, bezel, exponent) {
  const canvas = Object.assign(document.createElement("canvas"), { width, height });
  const context = canvas.getContext("2d");
  const image = context.createImageData(width, height);
  const data = image.data;
  const r = Math.max(radius, 0.001);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const sx = Math.sign(x + 0.5 - width / 2);
      const sy = Math.sign(y + 0.5 - height / 2);
      const px = Math.abs(x + 0.5 - width / 2) - (width / 2 - r);
      const py = Math.abs(y + 0.5 - height / 2) - (height / 2 - r);
      let nx;
      let ny;
      let distance;
      if (px > 0 && py > 0) {
        distance = Math.pow(px ** exponent + py ** exponent, 1 / exponent) - r;
        const gx = px ** (exponent - 1);
        const gy = py ** (exponent - 1);
        const length = Math.hypot(gx, gy);
        nx = gx / length;
        ny = gy / length;
      } else if (px > py) {
        distance = px - r;
        nx = 1;
        ny = 0;
      } else {
        distance = py - r;
        nx = 0;
        ny = 1;
      }
      const edge = Math.min(Math.max(1 + distance / bezel, 0), 1);
      const pull = 1 - Math.sqrt(1 - edge * edge);
      const i = (y * width + x) * 4;
      data[i] = 128 - sx * nx * pull * 127;
      data[i + 1] = 128 - sy * ny * pull * 127;
      data[i + 2] = 128;
      data[i + 3] = 255;
    }
  }
  context.putImageData(image, 0, 0);
  return canvas.toDataURL();
}

function dropLens(element) {
  const lens = lenses.get(element);
  if (!lens) return;
  lens.filter.remove();
  lenses.delete(element);
  element.style.removeProperty("backdrop-filter");
  element.style.removeProperty("-webkit-backdrop-filter");
}

function refract(element) {
  const style = getComputedStyle(element);
  if (!refracts || style.getPropertyValue("--penna-glass").trim() !== "1") {
    dropLens(element);
    return;
  }
  const width = Math.round(element.offsetWidth);
  const height = Math.round(element.offsetHeight);
  const shorter = Math.min(width, height);
  const bezel = Math.min(parseFloat(style.getPropertyValue("--penna-glass-bezel")) || 0, shorter / 4);
  const refraction = Math.min(parseFloat(style.getPropertyValue("--penna-glass-refraction")) || 0, shorter / 2);
  if (!width || !height || bezel <= 0 || refraction === 0) {
    dropLens(element);
    return;
  }
  const corner = parseFloat(style.getPropertyValue("--penna-corner"));
  const radius = Math.min(Number.isFinite(corner) ? corner : parseFloat(style.borderTopLeftRadius) || 0, shorter / 2);
  const exponent = 2 ** (parseFloat(style.getPropertyValue("--penna-k")) || 1);
  const backdrop = style.getPropertyValue("--penna-backdrop").trim();
  const key = [width, height, radius, bezel, refraction, exponent, backdrop].join();
  let lens = lenses.get(element);
  if (lens?.key === key) return;
  if (!lensRoot) {
    lensRoot = document.createElementNS(SVG, "svg");
    lensRoot.setAttribute("width", "0");
    lensRoot.setAttribute("height", "0");
    lensRoot.style.position = "absolute";
    document.body.append(lensRoot);
  }
  if (!lens) {
    const filter = document.createElementNS(SVG, "filter");
    filter.id = `penna-lens-${++lensCount}`;
    filter.setAttribute("x", "0");
    filter.setAttribute("y", "0");
    filter.setAttribute("width", "100%");
    filter.setAttribute("height", "100%");
    filter.setAttribute("color-interpolation-filters", "sRGB");
    filter.innerHTML = `<feImage x="0" y="0" result="map"/><feDisplacementMap in="SourceGraphic" in2="map" xChannelSelector="R" yChannelSelector="G"/>`;
    lensRoot.append(filter);
    lens = { filter };
    lenses.set(element, lens);
  }
  lens.key = key;
  const map = lens.filter.querySelector("feImage");
  map.setAttribute("width", width);
  map.setAttribute("height", height);
  map.setAttribute("href", lensMap(width, height, radius, bezel, exponent));
  lens.filter.querySelector("feDisplacementMap").setAttribute("scale", 2 * refraction);
  const lensed = /^blur\([^)]*\)/.test(backdrop)
    ? backdrop.replace(/^(blur\([^)]*\))/, `$1 url(#${lens.filter.id})`)
    : `url(#${lens.filter.id}) ${backdrop}`;
  element.style.webkitBackdropFilter = lensed;
  element.style.backdropFilter = lensed;
}

function settle(element) {
  round(element);
  refract(element);
}

const resized = new ResizeObserver((entries) => entries.forEach((entry) => settle(entry.target)));

function watch(root) {
  if (root.matches?.("[data-penna]")) resized.observe(root);
  root.querySelectorAll?.("[data-penna]").forEach((element) => resized.observe(element));
}

function roundAll(root = document) {
  if (root.matches?.("[data-penna]")) settle(root);
  root.querySelectorAll?.("[data-penna]").forEach(settle);
}

function switcher() {
  if (!showSwitcher || document.querySelector("[data-penna-switcher]")) return;
  const themes = THEMES.includes(choice.theme) ? THEMES : [...THEMES, choice.theme];
  const bar = document.createElement("div");
  bar.dataset.pennaSwitcher = "";
  const select = (label, names) => {
    const element = document.createElement("select");
    element.setAttribute("aria-label", label);
    element.append(...names.map((name) => new Option(name)));
    return element;
  };
  const theme = select("Theme", themes);
  const mode = select("Mode", MODES);
  bar.append(theme, mode);
  theme.value = choice.theme;
  mode.value = choice.mode;
  theme.onchange = () => {
    choice.theme = theme.value;
    apply();
  };
  mode.onchange = () => {
    choice.mode = mode.value;
    apply();
  };
  document.addEventListener("penna-theme", (event) => {
    theme.value = event.detail.theme;
    mode.value = event.detail.mode;
  });
  document.body.append(bar);
}

apply();
addEventListener("DOMContentLoaded", () => {
  watch(document);
  new MutationObserver((records) =>
    records.forEach((record) => {
      if (record.type === "attributes") {
        if (record.attributeName === "data-look" || record.attributeName === "data-ground") {
          roundAll(record.target);
        } else if (record.target.matches("[data-penna]")) {
          settle(record.target);
        }
        return;
      }
      record.addedNodes.forEach((node) => node.nodeType === 1 && watch(node));
    }),
  ).observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["data-state", "data-penna", "data-tone", "data-prominent", "data-look", "data-emphasis", "data-solid", "data-ground", "data-size", "data-shape", "aria-selected", "aria-current"],
  });
  switcher();
  roundAll();
  ceilTextRuns();
  document.fonts?.ready.then(() => ceilTextRuns());
});

function drawnCorner(corner, width, height, k = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-window-k")) || 1) {
  if (corner >= 9999) return [9999, 1];
  return apparentCorner(corner, Math.min(width, height) / 2, k);
}

function setDensity(next) {
  const valid = next === "compact" || next === "comfortable" ? next : "";
  if (valid) document.documentElement.dataset.density = valid;
  else delete document.documentElement.dataset.density;
  if (!params.has("bare")) remember({ ...stored(), density: valid || "regular" });
  requestAnimationFrame(() => {
    roundAll();
    ceilTextRuns();
  });
  document.dispatchEvent(new CustomEvent("penna-theme", { detail: { ...choice, density: valid || "regular" } }));
}

function setGlass(on) {
  if (on) delete document.documentElement.dataset.glass;
  else document.documentElement.dataset.glass = "none";
  if (!params.has("bare")) remember({ ...stored(), glass: on ? "liquid" : "none" });
  requestAnimationFrame(() => roundAll());
  document.dispatchEvent(new CustomEvent("penna-theme", { detail: { ...choice, glass: on ? "liquid" : "none" } }));
}

function setTheme(next) {
  const valid = themeName(next);
  if (!valid || valid === choice.theme) return;
  choice.theme = valid;
  apply();
}

function setMotion(off) {
  if (off) document.documentElement.dataset.motion = "off";
  else delete document.documentElement.dataset.motion;
  if (!params.has("bare")) remember({ ...stored(), motion: off ? "off" : "default" });
  document.dispatchEvent(new CustomEvent("penna-theme", { detail: { ...choice, motion: off ? "off" : "default" } }));
}

window.pennaKit = { choice, apply, roundAll, settle, setTheme, setDensity, setGlass, setMotion, ceilText: ceilTextRuns, focusRing, apparentCorner, drawnCorner };
