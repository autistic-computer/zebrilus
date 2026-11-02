(() => {
function ceilSpans(root, selector) {
  const spans = [...root.querySelectorAll(selector)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
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
  ceilSpans(slider, ".penna-steps-label, .penna-steps-value");
}

function pickStep(slider, index) {
  const count = stepLabels(slider).length;
  if (!count) return;
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
  slider.tabIndex = 0;
  slider.setAttribute("role", "slider");
  slider.setAttribute("aria-label", slider.dataset.label || "");
  slider.setAttribute("aria-valuemin", "0");
  slider.setAttribute("aria-valuemax", String(Math.max(labels.length - 1, 0)));
  slider.dataset.id ||= `slider:${slider.dataset.label || ""}`;
  const label = document.createElement("span");
  label.className = "penna-steps-label";
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
      if (event.button !== 0) return;
      event.preventDefault();
      slider.focus();
    });
    pill.addEventListener("click", () => {
      slider.focus();
      pickStep(slider, index);
    });
    pills.append(pill);
  });
  const value = document.createElement("span");
  value.className = "penna-steps-value";
  slider.replaceChildren(label, pills, value);
  slider.addEventListener("keydown", (event) => {
    const at = Number.parseInt(slider.dataset.value, 10) || 0;
    const moves = { ArrowLeft: at - 1, ArrowDown: at - 1, ArrowRight: at + 1, ArrowUp: at + 1, Home: 0, End: labels.length - 1 };
    if (!(event.key in moves)) return;
    event.preventDefault();
    pickStep(slider, moves[event.key]);
  });
  paintSteps(slider);
}

function settleSteps() {
  document.querySelectorAll(".penna-steps").forEach((slider) => {
    wireSteps(slider);
    paintSteps(slider);
  });
}

settleSteps();
document.fonts.ready.then(settleSteps);
})();
