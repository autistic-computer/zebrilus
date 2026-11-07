(() => {
const RING = 1.5;
const CORNER = 0.45;
const KEYS = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 };

function shape(mark) {
  const side = mark.offsetWidth;
  if (!side) return;
  const filled = mark.hasAttribute("data-on") || mark.hasAttribute("data-mixed");
  const gap = mark.dataset.tone ? RING : parseFloat(getComputedStyle(mark).getPropertyValue("--penna-detail-ring-gap")) || 0;
  const reach = RING + (filled ? gap : 0);
  const border = parseFloat(getComputedStyle(mark).borderTopWidth) || 0;
  mark.style.setProperty("--penna-ring-inset", `${-(reach + border)}px`);
  if (mark.dataset.kind === "radio") return;
  const quarter = (parseFloat(getComputedStyle(mark).getPropertyValue("--penna-radius")) || 0) / 4;
  const radius = Math.min(quarter, side / 4);
  const cap = side * CORNER;
  const ring = (side + 2 * reach) * CORNER;
  const k = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-window-k")) || 1;
  const drawn = (corner, shorter) => window.pennaKit?.apparentCorner(corner, shorter / 2, k) ?? [corner, k];
  const [box, boxK] = drawn(cap * Math.tanh(radius / cap), side);
  const [ringBox, ringK] = drawn(ring * Math.tanh((radius + reach) / ring), side + 2 * reach);
  mark.style.setProperty("--penna-check-r", `${box.toFixed(2)}px`);
  mark.style.setProperty("--penna-check-k", `superellipse(${boxK.toFixed(3)})`);
  mark.style.setProperty("--penna-ring-r", `${ringBox.toFixed(2)}px`);
  mark.style.setProperty("--penna-ring-k", `${ringK.toFixed(3)}`);
}

function markOf(control, kind) {
  let mark = control.querySelector(":scope > .penna-check");
  if (!mark) {
    mark = document.createElement("span");
    mark.className = "penna-check";
    control.prepend(mark);
  }
  if (kind) mark.dataset.kind = kind;
  return mark;
}

function paint(mark, { on, mixed, tone, size, disabled }) {
  mark.toggleAttribute("data-on", on);
  mark.toggleAttribute("data-mixed", mixed);
  mark.toggleAttribute("data-disabled", disabled);
  if (tone) mark.dataset.tone = tone;
  else delete mark.dataset.tone;
  if (size) mark.dataset.size = size;
  else delete mark.dataset.size;
  const icon = mark.dataset.kind === "radio" ? null : mixed ? "minus" : on ? "check" : null;
  let glyph = mark.querySelector(":scope > i[data-icon]");
  if (!icon) glyph?.remove();
  else {
    glyph ??= mark.appendChild(document.createElement("i"));
    glyph.dataset.icon = icon;
  }
  shape(mark);
}

function disabled(control) {
  return control.getAttribute("aria-disabled") === "true";
}

function focusable(control) {
  if (disabled(control)) control.removeAttribute("tabindex");
  else control.tabIndex = 0;
}

function hold(control) {
  control.addEventListener("mousedown", (event) => {
    if (event.button === 0) event.preventDefault();
  });
}

function plain(event) {
  return !(event.altKey || event.ctrlKey || event.metaKey || event.shiftKey);
}

function syncCheckbox(box) {
  const state = box.getAttribute("aria-checked");
  paint(markOf(box), {
    on: state === "true",
    mixed: state === "mixed",
    tone: box.dataset.tone,
    size: box.dataset.size,
    disabled: disabled(box),
  });
  focusable(box);
}

function wireCheckbox(box) {
  if (box.pennaCheckbox) return;
  box.pennaCheckbox = true;
  box.setAttribute("role", "checkbox");
  if (!box.hasAttribute("aria-checked")) box.setAttribute("aria-checked", "false");
  const flip = () => {
    if (disabled(box)) return;
    const checked = box.getAttribute("aria-checked") !== "true";
    box.setAttribute("aria-checked", String(checked));
    syncCheckbox(box);
    box.dispatchEvent(new CustomEvent("change", { detail: { checked } }));
  };
  hold(box);
  box.addEventListener("click", flip);
  box.addEventListener("keydown", (event) => {
    if (!plain(event) || (event.key !== " " && event.key !== "Enter")) return;
    event.preventDefault();
    if (!event.repeat) flip();
  });
  syncCheckbox(box);
}

function radios(group) {
  return [...group.querySelectorAll(":scope > .penna-radio")];
}

function picked(group) {
  const value = Number.parseInt(group.dataset.value ?? "", 10);
  return value >= 0 && value < radios(group).length ? value : null;
}

function syncGroup(group) {
  const at = picked(group);
  radios(group).forEach((radio, index) => {
    radio.setAttribute("role", "radio");
    radio.setAttribute("aria-checked", String(index === at));
    radio.toggleAttribute("data-ringed", index === (at ?? 0));
    paint(markOf(radio, "radio"), {
      on: index === at,
      mixed: false,
      tone: group.dataset.tone,
      size: group.dataset.size,
      disabled: disabled(group),
    });
  });
  focusable(group);
}

function pick(group, index) {
  if (disabled(group) || index === picked(group)) return;
  group.dataset.value = String(index);
  syncGroup(group);
  group.dispatchEvent(new CustomEvent("change", { detail: { index } }));
}

function wireGroup(group) {
  if (group.pennaRadios) return;
  group.pennaRadios = true;
  group.setAttribute("role", "radiogroup");
  hold(group);
  group.addEventListener("keydown", (event) => {
    if (!plain(event) || disabled(group)) return;
    const last = radios(group).length - 1;
    const at = picked(group);
    const ringed = at ?? 0;
    const next =
      event.key === "Home" ? 0
      : event.key === "End" ? last
      : event.key in KEYS ? Math.min(Math.max(ringed + KEYS[event.key], 0), last)
      : (event.key === " " || event.key === "Enter") && at === null ? ringed
      : null;
    if (next === null || last < 0) return;
    event.preventDefault();
    pick(group, next);
  });
  radios(group).forEach((radio, index) => radio.addEventListener("click", () => pick(group, index)));
  syncGroup(group);
}

function settleChecks() {
  document.querySelectorAll(".penna-checkbox").forEach(wireCheckbox);
  document.querySelectorAll(".penna-radio-group").forEach(wireGroup);
  document.querySelectorAll(".penna-check").forEach(shape);
}

settleChecks();
document.fonts.ready.then(settleChecks);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settleChecks(), true);
})();
