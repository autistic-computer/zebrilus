function snapText(selector, root = document) {
  const spans = [...root.querySelectorAll(selector)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function stepTabs(event) {
  const tab = event.target.closest?.(".penna-tab");
  if (!tab) return;
  const list = tab.closest("[role=tablist]");
  if (!list) return;
  const live = [...list.querySelectorAll(".penna-tab[tabindex]")];
  const at = live.indexOf(tab);
  const keys = { ArrowRight: 1, ArrowLeft: -1, Home: -Infinity, End: Infinity };
  if (!(event.key in keys) || at < 0) return;
  event.preventDefault();
  const step = keys[event.key];
  const next = Number.isFinite(step) ? live[(at + step + live.length) % live.length] : step < 0 ? live[0] : live.at(-1);
  next.focus();
}

const snapTabs = () => snapText(".penna-tab .penna-text");
snapTabs();
document.fonts.ready.then(snapTabs);
document.addEventListener("keydown", stepTabs);

function markFocus(event) {
  const target = event.target;
  if (!(target instanceof Element) || !target.matches("[data-penna][tabindex]:not([data-still])")) return;
  const states = new Set((target.dataset.state ?? "").split(/\s+/).filter(Boolean));
  if (event.type === "focusin" && target.matches(":focus-visible") && !states.has("focused")) {
    states.add("focused");
    target.dataset.lit = "";
  } else if (event.type === "focusout" && "lit" in target.dataset) {
    states.delete("focused");
    delete target.dataset.lit;
  } else return;
  if (states.size) target.dataset.state = [...states].join(" ");
  else target.removeAttribute("data-state");
}

document.addEventListener("focusin", markFocus);
document.addEventListener("focusout", markFocus);
