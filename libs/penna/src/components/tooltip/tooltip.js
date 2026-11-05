(() => {
  const DELAY = 500;
  const OFFSET = 1;

  function run(text, className) {
    const span = document.createElement("span");
    if (className) span.className = className;
    span.setAttribute("data-penna-text", "");
    span.textContent = text;
    return span;
  }

  function tipFor(target) {
    const framed = target.closest("section[data-section]");
    const host = framed ? (target.closest("[data-tooltip-host]") ?? target.parentElement) : document.body;
    let tip = host?.querySelector(`:scope > .penna-tooltip[data-floating]${framed ? "" : "[data-fixed]"}`);
    const label = target.dataset.tooltip;
    if (!label) return tip;
    if (!tip) {
      tip = document.createElement("div");
      tip.className = "penna-tooltip";
      tip.setAttribute("data-penna", "tooltip");
      tip.setAttribute("data-floating", "");
      tip.setAttribute("role", "tooltip");
      tip.toggleAttribute("data-fixed", !framed);
      host.append(tip);
    }
    const key = target.dataset.tooltipKey;
    tip.replaceChildren(run(label), ...(key ? [run(globalThis.pennaKbd?.text(key) ?? key, "penna-tooltip-key")] : []));
    return tip;
  }

  function fit(start, mouse, size, end) {
    if (start + size <= end) return start;
    const flipped = mouse - size - OFFSET;
    return flipped >= 0 ? flipped : 0;
  }

  function place(tip, x, y) {
    const fixed = tip.hasAttribute("data-fixed");
    const host = tip.offsetParent ?? tip.parentElement;
    const origin = fixed ? { left: 0, top: 0 } : host.getBoundingClientRect();
    const frame = fixed
      ? { left: 0, top: 0, width: innerWidth, height: innerHeight }
      : (tip.closest("section[data-section]") ?? document.documentElement).getBoundingClientRect();
    const box = tip.getBoundingClientRect();
    const style = getComputedStyle(tip);
    const width = box.width + parseFloat(style.marginLeft) + parseFloat(style.marginRight);
    const height = box.height + parseFloat(style.marginTop) + parseFloat(style.marginBottom);
    const left = frame.left + fit(x - frame.left + OFFSET, x - frame.left, width, frame.width);
    const top = frame.top + fit(y - frame.top + OFFSET, y - frame.top, height, frame.height);
    tip.style.left = `${left - origin.left}px`;
    tip.style.top = `${top - origin.top}px`;
  }

  let timer;
  let shown;

  function hide(target) {
    clearTimeout(timer);
    shown?.removeAttribute("data-open");
    if (target) tipFor(target)?.removeAttribute("data-open");
    shown = undefined;
  }

  document.addEventListener("pointerover", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (!target || target.contains(event.relatedTarget)) return;
    const tip = tipFor(target);
    if (!tip) return;
    hide();
    const { clientX: x, clientY: y } = event;
    timer = setTimeout(() => {
      tip.setAttribute("data-open", "");
      window.pennaKit?.ceilText(tip);
      place(tip, x, y);
      shown = tip;
    }, DELAY);
  });

  document.addEventListener("pointerout", (event) => {
    const target = event.target.closest("[data-tooltip]");
    if (!target || target.contains(event.relatedTarget)) return;
    if (target.hasAttribute("data-tooltip-pinned")) return;
    hide(target);
  });

  document.addEventListener("pointerdown", (event) => {
    const target = event.target.closest("[data-tooltip]:not([data-tooltip-pinned])");
    if (target) hide(target);
  });

  let focused;

  function unfocus() {
    if (focused && shown === focused) {
      focused.removeAttribute("data-open");
      shown = undefined;
    }
    focused = undefined;
  }

  document.addEventListener("focusin", (event) => {
    const target = event.target;
    if (!(target instanceof Element) || !target.matches("[data-tooltip]:focus-visible")) return;
    const tip = tipFor(target);
    if (!tip) return;
    hide();
    tip.setAttribute("data-open", "");
    window.pennaKit?.ceilText(tip);
    const box = target.getBoundingClientRect();
    place(tip, box.left + box.width / 2, box.bottom);
    shown = focused = tip;
  });

  document.addEventListener("focusout", unfocus);
  document.addEventListener("pointermove", unfocus);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") unfocus();
  });
})();
