(() => {
const THEMES = [
  ["default", "Default"],
  ["macos", "macOS"],
  ["yaru", "Yaru"],
];
const HUE_STOPS = 12;
const RING = 3.5;

function hueColor(hue) {
  const angle = (hue * Math.PI) / 180;
  const [l, a, b] = [0.72, 0.13 * Math.cos(angle), 0.13 * Math.sin(angle)];
  const cube = (value) => value ** 3;
  const lms = [
    cube(l + 0.3963377774 * a + 0.2158037573 * b),
    cube(l - 0.1055613458 * a - 0.0638541728 * b),
    cube(l - 0.0894841775 * a - 1.291485548 * b),
  ];
  const linear = [
    4.0767416621 * lms[0] - 3.3077115913 * lms[1] + 0.2309699292 * lms[2],
    -1.2684380046 * lms[0] + 2.6097574011 * lms[1] - 0.3413193965 * lms[2],
    -0.0041960863 * lms[0] - 0.7034186147 * lms[1] + 1.707614701 * lms[2],
  ];
  const encode = (value) => {
    const clipped = Math.min(Math.max(value, 0), 1);
    return clipped <= 0.0031308 ? 12.92 * clipped : 1.055 * clipped ** (1 / 2.4) - 0.055;
  };
  return `rgb(${linear.map((value) => (encode(value) * 255).toFixed(2)).join(" ")})`;
}
const rootStyle = () => getComputedStyle(document.documentElement);
const mode = () => document.documentElement.dataset.pennaMode || "dark";
const themeId = () => document.documentElement.dataset.pennaTheme || "default";
const px = (value) => parseFloat(value) || 0;

function ceilSpans(root, selector) {
  const spans = [...root.querySelectorAll(selector)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function adaptive(radius, width, height) {
  const cap = Math.min(width, height) * 0.45;
  return cap > 0 ? cap * Math.tanh(radius / cap) : 0;
}

function controlRadius() {
  const radius = px(rootStyle().getPropertyValue("--penna-radius"));
  return radius > 24 ? 9999 : radius;
}

function shapeRuns(root) {
  root.querySelectorAll(".appearance-run").forEach((run) => {
    const entries = run.querySelectorAll(".appearance-entry");
    if (!entries.length) return;
    const radius = px(getComputedStyle(run).getPropertyValue("--penna-r"));
    const width = run.offsetWidth;
    const first = entries[0].offsetHeight;
    const last = entries[entries.length - 1].offsetHeight;
    run.style.setProperty("--appearance-r-top", `${adaptive(radius, width, first)}px`);
    run.style.setProperty("--appearance-r-bottom", `${adaptive(radius, width, last)}px`);
  });
}

function themeSmoothing() {
  const probe = document.createElement("span");
  probe.dataset.penna = "pane";
  probe.hidden = true;
  document.body.append(probe);
  const smoothing = getComputedStyle(probe).getPropertyValue("--penna-style-k").trim();
  probe.remove();
  return smoothing || "1";
}

function edgeWidth() {
  const probe = document.createElement("i");
  probe.dataset.penna = "card";
  probe.hidden = true;
  document.body.append(probe);
  const width = px(getComputedStyle(probe).getPropertyValue("--penna-bw"));
  probe.remove();
  return width;
}

function holdHover(root) {
  const run = root.querySelector(".appearance-run");
  if (!run) return;
  const probe = document.createElement("span");
  probe.dataset.penna = "field";
  probe.dataset.state = "hover";
  probe.hidden = true;
  run.append(probe);
  root.style.setProperty("--appearance-field-hover", getComputedStyle(probe).getPropertyValue("--penna-fill").trim());
  probe.remove();
}

function shapeSwatches(root) {
  const radius = controlRadius();
  const swatch = radius >= 9999 ? 9 : adaptive(radius, 18, 18);
  const ring = radius > 0 ? (radius >= 9999 ? 9999 : adaptive(radius + RING, 25, 25)) : 0;
  const dot = adaptive(Math.min(radius, 5), 10, 10);
  root.style.setProperty("--appearance-k", themeSmoothing());
  root.style.setProperty("--appearance-edge", `${edgeWidth()}px`);
  root.style.setProperty("--appearance-swatch-r", `${swatch}px`);
  root.style.setProperty("--appearance-ring-r", `${ring}px`);
  root.style.setProperty("--appearance-dot-r", `${dot}px`);
  root.querySelectorAll(".appearance-swatch[data-light]").forEach((element) => {
    element.style.setProperty("--appearance-swatch", element.dataset[mode() === "dark" ? "dark" : "light"]);
  });
}

function decimals(step) {
  const text = String(step);
  return text.includes(".") ? text.split(".")[1].length : 0;
}

function snapValue(slider, value) {
  const min = Number(slider.dataset.min);
  const max = Number(slider.dataset.max);
  const step = Number(slider.dataset.step);
  const clamped = Math.min(Math.max(value, min), max);
  const snapped = step > 0 ? min + Math.round((clamped - min) / step) * step : clamped;
  return Number(Math.min(Math.max(snapped, min), max).toFixed(decimals(step)));
}

function display(slider, value) {
  switch (slider.dataset.format) {
    case "deg":
      return `${value.toFixed(0)}°`;
    case "px":
      return `${value.toFixed(0)}px`;
    default:
      return `${(value * 100).toFixed(0)}%`;
  }
}

function paintSlider(slider) {
  const min = Number(slider.dataset.min);
  const max = Number(slider.dataset.max);
  const value = Number(slider.dataset.value);
  const fraction = max > min ? Math.min(Math.max((value - min) / (max - min), 0), 1) : 0;
  slider.style.setProperty("--fraction", String(fraction));
  slider.setAttribute("aria-valuemin", String(min));
  slider.setAttribute("aria-valuemax", String(max));
  slider.setAttribute("aria-valuenow", String(value));
  slider.setAttribute("aria-valuetext", display(slider, value));
  const row = slider.closest(".appearance-slider-row");
  const shown = row?.querySelector(".appearance-shown");
  if (shown) {
    shown.textContent = display(slider, value);
    ceilSpans(row, ".appearance-shown");
  }
  if (slider.hasAttribute("data-hue")) {
    const stops = Array.from({ length: HUE_STOPS + 1 }, (_, stop) => hueColor((stop * 360) / HUE_STOPS));
    slider.style.setProperty("--appearance-hues", `linear-gradient(90deg, ${stops.join(", ")})`);
    row?.style.setProperty("--appearance-hue", hueColor(value));
  }
}

function setSlider(slider, value) {
  const next = snapValue(slider, value);
  if (String(next) === slider.dataset.value) return;
  slider.dataset.value = String(next);
  paintSlider(slider);
  slider.dispatchEvent(new CustomEvent("change", { detail: { value: next } }));
}

function valueAt(slider, clientX) {
  const box = slider.getBoundingClientRect();
  const radius = 8;
  const width = Math.max(box.width - radius * 2, 1);
  const fraction = Math.min(Math.max((clientX - box.left - radius) / width, 0), 1);
  const min = Number(slider.dataset.min);
  const max = Number(slider.dataset.max);
  return min + fraction * (max - min);
}

function wireSlider(slider) {
  if (slider.appearanceSlider) return;
  slider.appearanceSlider = true;
  slider.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    slider.focus({ focusVisible: false });
    slider.setPointerCapture(event.pointerId);
    setSlider(slider, valueAt(slider, event.clientX));
  });
  slider.addEventListener("pointermove", (event) => {
    if (!slider.hasPointerCapture(event.pointerId)) return;
    setSlider(slider, valueAt(slider, event.clientX));
  });
  slider.addEventListener("keydown", (event) => {
    const step = Number(slider.dataset.step) || (Number(slider.dataset.max) - Number(slider.dataset.min)) / 100;
    const steps = event.shiftKey ? 10 : 1;
    const value = Number(slider.dataset.value);
    const moves = {
      ArrowLeft: value - step * steps,
      ArrowDown: value - step * steps,
      ArrowRight: value + step * steps,
      ArrowUp: value + step * steps,
      Home: Number(slider.dataset.min),
      End: Number(slider.dataset.max),
    };
    if (!(event.key in moves)) return;
    event.preventDefault();
    setSlider(slider, moves[event.key]);
  });
}

function settleSliders(root) {
  root.querySelectorAll(".appearance-slider").forEach((slider) => {
    if (slider.dataset.value === "theme-radius" || slider.dataset.themeRadius !== undefined) {
      slider.dataset.themeRadius = "";
      slider.dataset.value = String(snapValue(slider, px(rootStyle().getPropertyValue("--penna-radius"))));
    }
    wireSlider(slider);
    paintSlider(slider);
  });
}

function wireSwatches(frame) {
  frame.querySelectorAll(".appearance-swatches").forEach((row) => {
    if (row.appearanceSwatches) return;
    row.appearanceSwatches = true;
    const pick = (swatch) => {
      row.querySelectorAll(".appearance-ring").forEach((ring) => ring.toggleAttribute("data-selected", ring.contains(swatch)));
      row.dispatchEvent(new CustomEvent("change", { bubbles: true, detail: { accent: swatch.dataset.accent } }));
    };
    row.querySelectorAll(".appearance-swatch").forEach((swatch) => {
      swatch.addEventListener("mousedown", (event) => event.button === 0 && event.preventDefault());
      swatch.addEventListener("click", () => pick(swatch));
      swatch.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        pick(swatch);
      });
    });
  });
}

function closeMenus(except) {
  document.querySelectorAll(".appearance-theme-menu").forEach((menu) => menu !== except && menu.remove());
  document.querySelectorAll(".appearance-theme[aria-expanded]").forEach((button) => {
    if (!except || !button.parentElement.contains(except)) button.removeAttribute("aria-expanded");
  });
}

function wireTheme(frame) {
  frame.querySelectorAll(".appearance-theme").forEach((button) => {
    if (button.appearanceTheme) return;
    button.appearanceTheme = true;
    const name = button.querySelector(".appearance-theme-name");
    button.dataset.picked ??= themeId();
    const open = () => {
      const menu = document.createElement("div");
      menu.className = "penna-menu appearance-theme-menu";
      menu.dataset.penna = "popover";
      menu.setAttribute("role", "menu");
      THEMES.forEach(([id, label]) => {
        const item = document.createElement("div");
        item.dataset.penna = "menu-item";
        item.setAttribute("role", "menuitemradio");
        item.setAttribute("aria-checked", String(id === button.dataset.picked));
        const check = document.createElement("i");
        check.className = "appearance-check";
        if (id === button.dataset.picked) check.dataset.icon = "check";
        const label_ = document.createElement("span");
        label_.className = "appearance-accent-label";
        const dot = document.createElement("span");
        dot.className = "appearance-dot";
        const text = document.createElement("span");
        text.className = "penna-label";
        text.textContent = label;
        label_.append(dot, text);
        item.append(check, label_);
        item.addEventListener("mouseenter", () => (item.dataset.state = "selected"));
        item.addEventListener("mouseleave", () => item.removeAttribute("data-state"));
        item.addEventListener("click", () => {
          button.dataset.picked = id;
          name.textContent = label;
          closeMenus();
          if (typeof snap === "function") snap(button.parentElement);
        });
        menu.append(item);
      });
      menu.addEventListener("mousedown", (event) => event.preventDefault());
      button.parentElement.append(menu);
      menu.toggleAttribute("data-glass", getComputedStyle(menu).getPropertyValue("--penna-backdrop").trim().startsWith("blur"));
      button.setAttribute("aria-expanded", "true");
      closeMenus(menu);
    };
    button.addEventListener("mousedown", (event) => {
      if (event.button !== 0) return;
      event.stopPropagation();
      if (button.hasAttribute("aria-expanded")) closeMenus();
      else open();
    });
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      if (event.detail === 0) {
        if (button.hasAttribute("aria-expanded")) closeMenus();
        else open();
      }
    });
    button.addEventListener("keydown", (event) => event.key === "Escape" && closeMenus());
  });
}

function nameTheme(frame) {
  frame.querySelectorAll(".appearance-theme").forEach((button) => {
    if (button.dataset.picked && button.dataset.picked !== button.dataset.followed) return;
    const id = themeId();
    button.dataset.picked = id;
    button.dataset.followed = id;
    button.querySelector(".appearance-theme-name").textContent = (THEMES.find(([each]) => each === id) ?? THEMES[0])[1];
  });
}

function pickMode(frame) {
  frame.querySelectorAll(".appearance-mode").forEach((control) => {
    if (control.dataset.touched) return;
    control.dataset.value = mode() === "dark" ? "2" : "1";
    if (!control.appearanceMode) {
      control.appearanceMode = true;
      control.addEventListener("change", () => (control.dataset.touched = ""));
    }
  });
}

function wireDisclosure(frame) {
  frame.querySelectorAll(".appearance-disclosure").forEach((button) => {
    if (button.appearanceDisclosure) return;
    button.appearanceDisclosure = true;
    button.addEventListener("click", () => {
      button.setAttribute("aria-expanded", String(button.getAttribute("aria-expanded") !== "true"));
      requestAnimationFrame(() => shapeRuns(frame));
    });
  });
}

function settleAppearance() {
  document.querySelectorAll(".appearance-frame").forEach((frame) => {
    pickMode(frame);
    nameTheme(frame);
    wireSwatches(frame);
    wireTheme(frame);
    wireDisclosure(frame);
    ceilSpans(frame, ".appearance-text, .appearance-title");
    settleSliders(frame);
    shapeSwatches(frame);
    holdHover(frame);
  });
  if (typeof snap === "function") snap();
  document.querySelectorAll(".appearance-frame").forEach(shapeRuns);
}

document.addEventListener("mousedown", (event) => {
  if (!event.target.closest?.(".appearance-theme-menu")) closeMenus();
});
settleAppearance();
document.fonts.ready.then(settleAppearance);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(settleAppearance), true);
})();
