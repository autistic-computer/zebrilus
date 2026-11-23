(() => {
const LIVE = '.penna-tab[role="tab"][tabindex]:not([data-still]):not([aria-disabled="true"])';

function liveTabs(list) {
  return [...list.querySelectorAll(LIVE)];
}

function setSelected(tab, selected) {
  const states = new Set((tab.dataset.state ?? "").split(/\s+/).filter(Boolean));
  if (selected) states.add("selected");
  else states.delete("selected");
  if (states.size) tab.dataset.state = [...states].join(" ");
  else tab.removeAttribute("data-state");
  tab.setAttribute("aria-selected", selected ? "true" : "false");
  tab.tabIndex = selected ? 0 : -1;
}

function selectTab(tab) {
  const list = tab.closest("[role=tablist]");
  if (!list) return;
  const live = liveTabs(list);
  if (!live.includes(tab)) return;
  live.forEach((item) => setSelected(item, item === tab));
  list.dispatchEvent(new CustomEvent("change", { detail: { tab }, bubbles: true }));
}

function stepTabs(event) {
  const tab = event.target.closest?.(LIVE);
  if (!tab) return;
  const list = tab.closest("[role=tablist]");
  if (!list) return;
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    selectTab(tab);
    return;
  }
  const live = liveTabs(list);
  const at = live.indexOf(tab);
  const keys = { ArrowRight: 1, ArrowLeft: -1, Home: -Infinity, End: Infinity };
  if (!(event.key in keys) || at < 0) return;
  event.preventDefault();
  const step = keys[event.key];
  const next = Number.isFinite(step) ? live[(at + step + live.length) % live.length] : step < 0 ? live[0] : live.at(-1);
  live.forEach((item) => {
    item.tabIndex = item === next ? 0 : -1;
  });
  next.focus();
}

function clickTab(event) {
  if (event.target.closest?.("button")) return;
  const tab = event.target.closest?.(LIVE);
  if (!tab) return;
  selectTab(tab);
}

function markFocus(event) {
  const target = event.target;
  if (!(target instanceof Element) || !target.matches(".penna-tab[data-penna][tabindex]:not([data-still])")) return;
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

document.addEventListener("click", clickTab);
document.addEventListener("keydown", stepTabs);
document.addEventListener("focusin", markFocus);
document.addEventListener("focusout", markFocus);
})();
