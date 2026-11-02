(() => {
function segmentLabels(control) {
  return [...control.querySelectorAll(".penna-segment > .penna-label")];
}

function ceilSegments(control) {
  const labels = segmentLabels(control);
  labels.forEach((label) => label.style.removeProperty("width"));
  const widths = labels.map((label) => label.getBoundingClientRect().width);
  labels.forEach((label, index) => {
    label.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function placePill(control) {
  const segments = [...control.querySelectorAll(".penna-segment")];
  const at = Math.min(Math.max(Number.parseInt(control.dataset.value ?? "0", 10) || 0, 0), segments.length - 1);
  const pill = control.querySelector(".penna-segment-pill");
  const target = segments[at];
  if (!pill || !target) return;
  segments.forEach((segment, index) => {
    segment.setAttribute("aria-selected", String(index === at));
  });
  pill.style.setProperty("--pill-x", `${target.offsetLeft}px`);
  pill.style.setProperty("--pill-w", `${target.offsetWidth}px`);
  const track = getComputedStyle(control);
  const outer = parseFloat(track.borderTopLeftRadius) || 0;
  const border = parseFloat(track.borderTopWidth) || 0;
  const radius = Math.max(Math.min(outer, control.offsetHeight / 2) - border - 2, 0);
  pill.style.borderRadius = `${radius}px`;
}

function wireSegmented(control) {
  if (control.pennaSegmented) return;
  control.pennaSegmented = true;
  control.querySelectorAll(".penna-segment").forEach((segment, index) => {
    segment.addEventListener("mousedown", (event) => {
      if (event.button === 0) event.preventDefault();
    });
    segment.addEventListener("click", () => {
      if (String(index) === control.dataset.value) return;
      control.dataset.value = String(index);
      placePill(control);
      control.dispatchEvent(new CustomEvent("change", { detail: { index } }));
    });
  });
}

function settleSegmented() {
  document.querySelectorAll(".penna-segmented").forEach((control) => {
    control.toggleAttribute("data-still", true);
    wireSegmented(control);
    ceilSegments(control);
    placePill(control);
    control.offsetWidth;
    requestAnimationFrame(() => control.removeAttribute("data-still"));
  });
}

settleSegmented();
document.fonts.ready.then(settleSegmented);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settleSegmented(), true);
})();
