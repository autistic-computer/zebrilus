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

const saved = stored();
const choice = {
  theme: themeName(params.get("theme")) || themeName(saved.theme) || script.dataset.theme || THEMES[0],
  mode: modeName(params.get("mode")) || modeName(saved.mode) || "dark",
};

function sheet(id) {
  let link = document.getElementById(id);
  if (!link) {
    link = document.createElement("link");
    link.id = id;
    link.rel = "stylesheet";
    document.head.prepend(link);
  }
  return link;
}

function load(id, file) {
  const link = sheet(id);
  const href = new URL(`${file}.css`, tokens).href;
  if (link.href !== href) {
    link.href = href;
    link.addEventListener("load", () => requestAnimationFrame(roundAll), { once: true });
  }
}

function apply() {
  load("penna-icons", `${choice.theme}-icons`);
  load("penna-tokens", `${choice.theme}-${choice.mode}`);
  document.documentElement.dataset.pennaTheme = choice.theme;
  document.documentElement.dataset.pennaMode = choice.mode;
  remember(choice);
}

function round(element) {
  const style = getComputedStyle(element);
  const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
  if (radius <= 0 || radius >= 9999) {
    element.style.removeProperty("border-radius");
    return;
  }
  const width = element.offsetWidth;
  const height = element.offsetHeight;
  if (!width || !height) return;
  let resolved;
  if (style.getPropertyValue("--penna-row").trim() === "1") {
    resolved = Math.min(radius, height * 0.4);
  } else {
    const cap = Math.min(width, height) * 0.45;
    resolved = cap * Math.tanh(radius / cap);
  }
  element.style.borderRadius = `${resolved.toFixed(2)}px`;
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
}

function refract(element) {
  const style = getComputedStyle(element);
  if (!refracts || style.getPropertyValue("--penna-glass").trim() !== "1") {
    dropLens(element);
    return;
  }
  const width = Math.round(element.offsetWidth);
  const height = Math.round(element.offsetHeight);
  const root = getComputedStyle(document.documentElement);
  const shorter = Math.min(width, height);
  const bezel = Math.min(parseFloat(root.getPropertyValue("--penna-glass-bezel")) || 0, shorter / 4);
  const refraction = Math.min(parseFloat(root.getPropertyValue("--penna-glass-refraction")) || 0, shorter / 2);
  if (!width || !height || bezel <= 0 || refraction === 0) {
    dropLens(element);
    return;
  }
  const radius = Math.min(parseFloat(style.borderTopLeftRadius) || 0, shorter / 2);
  const exponent = 2 ** (parseFloat(style.getPropertyValue("--penna-k")) || 1);
  const key = [width, height, radius, bezel, refraction, exponent].join();
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
  const backdrop = style.getPropertyValue("--penna-backdrop").trim();
  const lensed = /^blur\([^)]*\)/.test(backdrop)
    ? backdrop.replace(/^(blur\([^)]*\))/, `$1 url(#${lens.filter.id})`)
    : `url(#${lens.filter.id}) ${backdrop}`;
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

function roundAll() {
  document.querySelectorAll("[data-penna]").forEach(settle);
}

function switcher() {
  if (document.querySelector("[data-penna-switcher]") || params.has("bare")) return;
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
  document.body.append(bar);
}

apply();
addEventListener("DOMContentLoaded", () => {
  watch(document);
  new MutationObserver((records) =>
    records.forEach((record) => {
      if (record.type === "attributes") {
        if (record.target.matches("[data-penna]")) settle(record.target);
        return;
      }
      record.addedNodes.forEach((node) => node.nodeType === 1 && watch(node));
    }),
  ).observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["data-state", "data-penna", "data-tone", "data-prominent", "aria-selected", "aria-current"],
  });
  switcher();
  roundAll();
});

window.pennaKit = { choice, apply, roundAll };
