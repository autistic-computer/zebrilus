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
  rings();
}

function rings() {
  document.querySelectorAll('[data-penna][data-state~="focused"]:is(.penna-tab, .penna-row, .penna-item)').forEach((element) => {
    const style = getComputedStyle(element);
    const radius = parseFloat(style.getPropertyValue("--penna-r")) || 0;
    const box = element.getBoundingClientRect();
    const cap = (Math.min(box.width, box.height) + 3) * 0.45;
    const ring = radius >= 9999 ? 9999 : cap > 0 ? cap * Math.tanh((radius + 1.5) / cap) : 0;
    element.style.setProperty("--penna-ring-inset", `${-(1.5 + (parseFloat(style.borderTopWidth) || 0))}px`);
    element.style.setProperty("--penna-ring-r", `${ring}px`);
  });
}

rings();
document.fonts.ready.then(rings);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && rings(), true);

document.addEventListener("focusin", markFocus);
document.addEventListener("focusout", markFocus);
