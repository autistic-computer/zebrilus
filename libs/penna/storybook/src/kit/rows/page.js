const coarse = matchMedia("(pointer: coarse)");

function light(group, state) {
  const fill = group.querySelector(".penna-row-fill:not([data-still])");
  if (!fill) return;
  if (state) fill.dataset.state = state;
  else fill.removeAttribute("data-state");
}

document.querySelectorAll(".penna-fill-row").forEach((group) => {
  group.addEventListener("pointerenter", () => light(group, "hover"));
  group.addEventListener("pointerleave", () => light(group, ""));
  group.addEventListener("pointerdown", () => light(group, coarse.matches ? "hover active" : "hover"));
  group.addEventListener("pointerup", () => light(group, "hover"));
  group.addEventListener("focus", () => group.matches(":focus-visible") && light(group, "focused"));
  group.addEventListener("blur", () => light(group, ""));
});

function stepRows(event) {
  const row = event.target.closest?.(".penna-row, .penna-item, .penna-fill-row");
  if (!row) return;
  const list = row.parentElement;
  const live = [...list.querySelectorAll(":scope > [tabindex]")];
  const at = live.indexOf(row);
  const keys = { ArrowDown: 1, ArrowUp: -1 };
  if (!(event.key in keys) || at < 0 || live.length < 2) return;
  event.preventDefault();
  live[(at + keys[event.key] + live.length) % live.length].focus();
}

document.addEventListener("keydown", stepRows);
