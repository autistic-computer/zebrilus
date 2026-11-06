(() => {
const LOOKS = {
  default: { accent: "ink", controls: "0", glass: "true" },
  macos: { accent: "blue", controls: "1", glass: "true" },
  yaru: { accent: "orange", controls: "0", glass: "false" },
};
const HUE_STOPS = 12;
const RING = 3.5;
const MAX_CORNER_RADIUS = 25;

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
const look = () => LOOKS[themeId()] ?? LOOKS.default;
const px = (value) => parseFloat(value) || 0;

function adaptive(radius, width, height) {
  const cap = Math.min(width, height) * 0.45;
  return cap > 0 ? cap * Math.tanh(radius / cap) : 0;
}

function controlRadius() {
  return px(rootStyle().getPropertyValue("--penna-control-radius"));
}

function shapeRuns(root) {
  root.querySelectorAll(".appearance-run").forEach((run) => {
    const entries = run.querySelectorAll(".appearance-entry");
    if (!entries.length) return;
    const radius = px(getComputedStyle(run).getPropertyValue("--penna-r"));
    const width = run.offsetWidth;
    const first = entries[0].offsetHeight;
    const last = entries[entries.length - 1].offsetHeight;
    const drawn = (height) => window.pennaKit?.drawnCorner(adaptive(radius, width, height), width, height) ?? [adaptive(radius, width, height), 1];
    const [top, topK] = drawn(first);
    const [bottom] = drawn(last);
    run.style.setProperty("--appearance-r-top", `${top}px`);
    run.style.setProperty("--appearance-r-bottom", `${bottom}px`);
    run.style.setProperty("--appearance-run-k", `${topK}`);
  });
}

function shapeSwatches(root) {
  const radius = controlRadius();
  const swatch = radius >= 9999 ? 9999 : adaptive(radius, 18, 18);
  const ring = radius > 0 ? (radius >= 9999 ? 9999 : adaptive(radius + RING, 25, 25)) : 0;
  const [swatchBox, swatchK] = window.pennaKit?.drawnCorner(swatch, 18, 18) ?? [swatch, 1];
  const [ringBox, ringK] = window.pennaKit?.drawnCorner(ring, 25, 25) ?? [ring, 1];
  root.style.setProperty("--appearance-swatch-r", `${swatchBox}px`);
  root.style.setProperty("--appearance-ring-r", `${ringBox}px`);
  root.style.setProperty("--appearance-swatch-k", `${swatchK}`);
  root.style.setProperty("--appearance-ring-k", `${ringK}`);
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
    pennaKit.ceilText(shown);
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
    slider.removeAttribute("data-focused");
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
    slider.toggleAttribute("data-focused", true);
    setSlider(slider, moves[event.key]);
  });
  slider.addEventListener("blur", () => slider.removeAttribute("data-focused"));
}

function settleSliders(root) {
  root.querySelectorAll(".appearance-slider").forEach((slider) => {
    if (slider.dataset.value === "theme-radius" || slider.dataset.themeRadius !== undefined) {
      slider.dataset.themeRadius = "";
      const radius = Math.min(Math.max(px(rootStyle().getPropertyValue("--penna-radius")), 0), MAX_CORNER_RADIUS);
      slider.dataset.value = String(snapValue(slider, Math.sqrt(radius / MAX_CORNER_RADIUS)));
    }
    wireSlider(slider);
    paintSlider(slider);
  });
}

function wireCustom(frame) {
  frame.querySelectorAll(".appearance-swatches .penna-color-picker").forEach((picker) => {
    if (picker.appearanceCustom) return;
    picker.appearanceCustom = true;
    const row = picker.closest(".appearance-swatches");
    picker.addEventListener("change", (event) => {
      const custom = Boolean(event.detail);
      picker.toggleAttribute("data-custom", custom);
      row.querySelectorAll(".appearance-ring").forEach((ring) => {
        const own = ring.contains(picker);
        const ink = Boolean(ring.querySelector('[data-accent="ink"]'));
        ring.toggleAttribute("data-selected", custom ? own : ink);
      });
    });
  });
}

function wireSwatches(frame) {
  frame.querySelectorAll(".appearance-swatches").forEach((row) => {
    if (row.appearanceSwatches) return;
    row.appearanceSwatches = true;
    const pick = (swatch) => {
      row.querySelectorAll(".appearance-ring").forEach((ring) => ring.toggleAttribute("data-selected", ring.contains(swatch)));
      row.querySelectorAll(".penna-color-picker").forEach((picker) => picker.removeAttribute("data-custom"));
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

function followLook(frame) {
  const picked = look();
  frame.querySelectorAll(".appearance-swatches").forEach((row) => {
    if (row.dataset.touched !== undefined) return;
    row.querySelectorAll(".appearance-ring").forEach((ring) => {
      ring.toggleAttribute("data-selected", Boolean(ring.querySelector(`[data-accent="${picked.accent}"]`)));
    });
    if (!row.appearanceFollow) {
      row.appearanceFollow = true;
      row.addEventListener("change", () => (row.dataset.touched = ""));
    }
  });
  [
    [".appearance-look-controls", "data-value", picked.controls],
    [".appearance-look-glass", "aria-checked", picked.glass],
  ].forEach(([selector, attribute, value]) => {
    frame.querySelectorAll(selector).forEach((control) => {
      if (control.dataset.touched !== undefined) return;
      control.setAttribute(attribute, value);
      if (!control.appearanceFollow) {
        control.appearanceFollow = true;
        control.addEventListener("change", () => (control.dataset.touched = ""));
      }
    });
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

function settleAppearance() {
  document.querySelectorAll(".appearance-frame").forEach((frame) => {
    pickMode(frame);
    followLook(frame);
    wireSwatches(frame);
    wireCustom(frame);
    pennaKit.ceilText(frame);
    settleSliders(frame);
    shapeSwatches(frame);
  });
  if (typeof snap === "function") snap();
  document.querySelectorAll(".appearance-frame").forEach(shapeRuns);
}

settleAppearance();
document.fonts.ready.then(settleAppearance);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(settleAppearance), true);
})();
