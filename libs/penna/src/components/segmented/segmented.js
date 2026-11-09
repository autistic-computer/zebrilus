(() => {
const RING = 1.5;
const FULL = 9999;
const CORNER = 0.45;
const KEYS = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 };

function picked(control) {
  const count = control.querySelectorAll(".penna-segment").length;
  return Math.min(Math.max(Number.parseInt(control.dataset.value ?? "0", 10) || 0, 0), count - 1);
}

function placePill(control) {
  const segments = [...control.querySelectorAll(".penna-segment")];
  const at = picked(control);
  const pill = control.querySelector(".penna-segment-pill");
  const target = segments[at];
  if (!pill || !target) return;
  segments.forEach((segment, index) => {
    segment.setAttribute("aria-selected", String(index === at));
  });
  pill.style.setProperty("--pill-x", `${target.offsetLeft}px`);
  pill.style.setProperty("--pill-w", `${target.offsetWidth}px`);
  const track = getComputedStyle(control);
  const border = parseFloat(track.borderTopWidth) || 0;
  const inner = control.offsetHeight - 2 * border - 4;
  if ((parseFloat(track.getPropertyValue("--penna-r")) || 0) >= 9999) {
    control.style.setProperty("--penna-pill-r", "9999px");
    control.style.setProperty("--penna-pill-k", "1");
    return;
  }
  const outer = (parseFloat(track.getPropertyValue("--penna-share")) || 0) * control.offsetHeight;
  const roundness = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-control-share")) || 0;
  const radius = Math.max(Math.min(outer - border - 2, roundness * inner), 0);
  const k = parseFloat(track.getPropertyValue("--penna-k")) || 1;
  const [box, shape] = window.pennaKit?.apparentCorner(radius, inner / 2, k) ?? [radius, k];
  control.style.setProperty("--penna-pill-r", `${box}px`);
  control.style.setProperty("--penna-pill-k", `${shape}`);
}

function shapeRing(control) {
  const style = getComputedStyle(control);
  const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
  const reach = RING + (parseFloat(style.borderTopWidth) || 0);
  const box = control.getBoundingClientRect();
  const shorter = Math.min(box.width, box.height) + 2 * reach;
  const share = parseFloat(style.getPropertyValue("--penna-share")) || 0;
  const cap = shorter * CORNER;
  const ring =
    radius >= FULL ? FULL : share > 0 ? Math.min(share, 0.5) * shorter : cap > 0 ? cap * Math.tanh((radius + RING) / cap) : 0;
  const k = parseFloat(style.getPropertyValue("--penna-k")) || 1;
  const [drawn, shape] = ring >= FULL ? [FULL, 1] : window.pennaKit?.apparentCorner(ring, shorter / 2, k) ?? [ring, k];
  control.style.setProperty("--penna-ring-inset", `${-reach}px`);
  control.style.setProperty("--penna-ring-r", `${drawn}px`);
  control.style.setProperty("--penna-ring-k", `${shape}`);
}

function disabled(control) {
  return control.getAttribute("aria-disabled") === "true";
}

function syncDisabled(control) {
  const off = disabled(control);
  const segments = control.querySelector(".penna-segments");
  if (off) segments?.setAttribute("aria-disabled", "true");
  else segments?.removeAttribute("aria-disabled");
  if (off) control.removeAttribute("tabindex");
  else if (!control.hasAttribute("tabindex")) control.tabIndex = 0;
}

function pick(control, index) {
  if (disabled(control) || index === picked(control)) return;
  control.dataset.value = String(index);
  placePill(control);
  control.dispatchEvent(new CustomEvent("change", { detail: { index } }));
}

function wireSegmented(control) {
  if (control.pennaSegmented) return;
  control.pennaSegmented = true;
  control.setAttribute("role", "radiogroup");
  control.addEventListener("mousedown", (event) => {
    if (event.button === 0) event.preventDefault();
  });
  control.addEventListener("keydown", (event) => {
    if (disabled(control) || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const last = control.querySelectorAll(".penna-segment").length - 1;
    const at = picked(control);
    const next = event.key === "Home" ? 0 : event.key === "End" ? last : event.key in KEYS ? at + KEYS[event.key] : null;
    if (next === null) return;
    event.preventDefault();
    pick(control, Math.min(Math.max(next, 0), last));
  });
  control.querySelectorAll(".penna-segment").forEach((segment, index) => {
    segment.addEventListener("click", () => pick(control, index));
  });
}

function set(control, index) {
  control.dataset.value = String(index);
  placePill(control);
}

function settleSegmented(root = document) {
  root.querySelectorAll(".penna-segmented").forEach((control) => {
    control.toggleAttribute("data-still", true);
    wireSegmented(control);
    syncDisabled(control);
    pennaKit.ceilText(control);
    placePill(control);
    shapeRing(control);
    control.offsetWidth;
    requestAnimationFrame(() => control.removeAttribute("data-still"));
  });
}

settleSegmented();
document.fonts.ready.then(() => settleSegmented());
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settleSegmented(), true);

globalThis.pennaSegmented = { wire: settleSegmented, set };
})();
