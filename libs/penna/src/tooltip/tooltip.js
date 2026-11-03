(() => {
  const DELAY = 500;
  const OFFSET = 1;

  function tipFor(target) {
    const host = target.closest("[data-tooltip-host]") ?? target.parentElement;
    return host?.querySelector(":scope > .penna-tooltip[data-floating]");
  }

  function place(tip, x, y) {
    const host = tip.offsetParent ?? tip.parentElement;
    const origin = host.getBoundingClientRect();
    tip.style.left = `${x - origin.left + OFFSET}px`;
    tip.style.top = `${y - origin.top + OFFSET}px`;
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
      place(tip, x, y);
      tip.setAttribute("data-open", "");
      window.pennaKit?.ceilText(tip);
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
})();
