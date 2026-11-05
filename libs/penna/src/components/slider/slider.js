(() => {
function disabled(slider) {
  return slider.getAttribute("aria-disabled") === "true";
}

function syncFocus(slider) {
  if (disabled(slider)) {
    slider.removeAttribute("tabindex");
    slider.removeAttribute("data-focused");
    if (document.activeElement === slider) slider.blur();
  } else if (!slider.hasAttribute("tabindex")) {
    slider.tabIndex = 0;
  }
}

function stepLabels(slider) {
  return (slider.dataset.steps || "").split(",").map((name) => name.trim()).filter(Boolean);
}

function paintSteps(slider) {
  const labels = stepLabels(slider);
  const at = Math.min(Math.max(Number.parseInt(slider.dataset.value ?? "0", 10) || 0, 0), Math.max(labels.length - 1, 0));
  slider.dataset.value = String(at);
  slider.setAttribute("aria-valuetext", labels[at] ?? "");
  slider.setAttribute("aria-valuenow", String(at));
  slider.querySelectorAll(".penna-step").forEach((pill, index) => {
    pill.toggleAttribute("data-filled", index <= at);
    pill.toggleAttribute("data-picked", index === at);
  });
  const value = slider.querySelector(".penna-steps-value");
  value.textContent = labels[at] ?? "";
  syncFocus(slider);
  pennaKit.ceilText(slider);
}

function pickStep(slider, index) {
  const count = stepLabels(slider).length;
  if (!count || disabled(slider)) return;
  const next = Math.min(Math.max(index, 0), count - 1);
  if (String(next) === slider.dataset.value) return;
  slider.dataset.value = String(next);
  paintSteps(slider);
  slider.dispatchEvent(new CustomEvent("change", { detail: { index: next, value: stepLabels(slider)[next] } }));
}

function wireSteps(slider) {
  if (slider.pennaSteps) return;
  slider.pennaSteps = true;
  const labels = stepLabels(slider);
  slider.setAttribute("role", "slider");
  slider.setAttribute("aria-label", slider.dataset.label || "");
  slider.setAttribute("aria-valuemin", "0");
  slider.setAttribute("aria-valuemax", String(Math.max(labels.length - 1, 0)));
  slider.dataset.id ||= `slider:${slider.dataset.label || ""}`;
  const label = document.createElement("span");
  label.className = "penna-steps-label";
  label.dataset.pennaText = "";
  label.textContent = slider.dataset.label || "";
  const pills = document.createElement("span");
  pills.className = "penna-steps-pills";
  pills.style.setProperty("--count", String(labels.length));
  labels.forEach((name, index) => {
    const pill = document.createElement("span");
    pill.className = "penna-step";
    pill.setAttribute("role", "button");
    pill.setAttribute("aria-label", name);
    pill.title = name;
    pill.dataset.id = `button:${name}`;
    pill.addEventListener("mousedown", (event) => {
      if (event.button !== 0 || disabled(slider)) return;
      event.preventDefault();
      slider.focus({ focusVisible: false });
    });
    pill.addEventListener("click", () => {
      if (disabled(slider)) return;
      slider.focus({ focusVisible: false });
      pickStep(slider, index);
    });
    pills.append(pill);
  });
  const value = document.createElement("span");
  value.className = "penna-steps-value";
  value.dataset.pennaText = "";
  slider.replaceChildren(label, pills, value);
  slider.addEventListener("keydown", (event) => {
    if (disabled(slider)) return;
    const at = Number.parseInt(slider.dataset.value, 10) || 0;
    const moves = { ArrowLeft: at - 1, ArrowDown: at - 1, ArrowRight: at + 1, ArrowUp: at + 1, Home: 0, End: labels.length - 1 };
    if (!(event.key in moves)) return;
    event.preventDefault();
    slider.toggleAttribute("data-focused", true);
    pickStep(slider, moves[event.key]);
  });
  slider.addEventListener("mousedown", () => slider.removeAttribute("data-focused"));
  slider.addEventListener("blur", () => slider.removeAttribute("data-focused"));
  paintSteps(slider);
}

function settleSteps() {
  document.querySelectorAll(".penna-steps").forEach((slider) => {
    wireSteps(slider);
    paintSteps(slider);
  });
}

function bound(slider, key, fallback) {
  const value = Number.parseFloat(slider.dataset[key]);
  return Number.isFinite(value) ? value : fallback;
}

function decimals(step) {
  const text = String(step);
  return text.includes(".") ? text.split(".")[1].length : 0;
}

function snapValue(slider, value) {
  const min = bound(slider, "min", 0);
  const max = Math.max(bound(slider, "max", 100), min);
  const step = Math.max(bound(slider, "step", 0), 0);
  const clamped = Math.min(Math.max(value, min), max);
  const snapped = step > 0 ? min + Math.round((clamped - min) / step) * step : clamped;
  const exact = Math.min(Math.max(snapped, min), max);
  return step > 0 ? Number(exact.toFixed(decimals(step))) : exact;
}

function paintSlider(slider) {
  const min = bound(slider, "min", 0);
  const max = bound(slider, "max", 100);
  const value = snapValue(slider, bound(slider, "value", min));
  const fraction = max > min ? Math.min(Math.max((value - min) / (max - min), 0), 1) : 0;
  slider.dataset.value = String(value);
  slider.style.setProperty("--penna-value", `${fraction * 100}%`);
  slider.setAttribute("aria-valuemin", String(min));
  slider.setAttribute("aria-valuemax", String(max));
  slider.setAttribute("aria-valuenow", String(value));
  syncFocus(slider);
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
  const radius = parseFloat(getComputedStyle(slider).paddingLeft) || 0;
  const width = Math.max(box.width - radius * 2, 1);
  const fraction = Math.min(Math.max((clientX - box.left - radius) / width, 0), 1);
  const min = bound(slider, "min", 0);
  return min + fraction * (bound(slider, "max", 100) - min);
}

function wireSlider(slider) {
  if (slider.pennaSlider) return;
  slider.pennaSlider = true;
  slider.setAttribute("role", "slider");
  slider.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || disabled(slider)) return;
    event.preventDefault();
    slider.removeAttribute("data-focused");
    slider.focus({ focusVisible: false });
    slider.setPointerCapture(event.pointerId);
    setSlider(slider, valueAt(slider, event.clientX));
  });
  slider.addEventListener("pointermove", (event) => {
    if (!slider.hasPointerCapture(event.pointerId) || disabled(slider)) return;
    setSlider(slider, valueAt(slider, event.clientX));
  });
  slider.addEventListener("keydown", (event) => {
    if (disabled(slider)) return;
    const min = bound(slider, "min", 0);
    const max = bound(slider, "max", 100);
    const step = bound(slider, "step", 0) || (max - min) / 100;
    const nudge = step * (event.shiftKey ? 10 : 1);
    const value = bound(slider, "value", min);
    const moves = { ArrowLeft: value - nudge, ArrowDown: value - nudge, ArrowRight: value + nudge, ArrowUp: value + nudge, Home: min, End: max };
    if (!(event.key in moves)) return;
    event.preventDefault();
    slider.toggleAttribute("data-focused", true);
    setSlider(slider, moves[event.key]);
  });
  slider.addEventListener("blur", () => slider.removeAttribute("data-focused"));
}

function underShadow() {
  const lifted = getComputedStyle(document.documentElement).getPropertyValue("--penna-shadow-lifted");
  const color = lifted.match(/#[0-9a-f]{3,8}\b|rgba?\([^)]*\)/i)?.[0];
  document.querySelectorAll(".penna-slider").forEach((slider) => {
    if (color) slider.style.setProperty("--penna-slider-under", color);
    else slider.style.removeProperty("--penna-slider-under");
  });
}

function settleSliders() {
  document.querySelectorAll(".penna-slider").forEach((slider) => {
    wireSlider(slider);
    paintSlider(slider);
  });
  underShadow();
}

settleSteps();
settleSliders();
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && underShadow(), true);
})();
