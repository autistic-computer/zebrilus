(() => {
  function panelOf(popover) {
    return popover.querySelector(":scope > .penna-popover-panel");
  }

  function triggerOf(popover) {
    return popover.querySelector(":scope > .penna-popover-trigger");
  }

  function setOpen(popover, open) {
    const trigger = triggerOf(popover);
    const panel = panelOf(popover);
    popover.toggleAttribute("data-open", open);
    if (trigger) {
      trigger.setAttribute("aria-expanded", String(open));
      const states = (trigger.dataset.state ?? "").split(/\s+/).filter((state) => state && state !== "current");
      if (open) states.push("current");
      if (states.length) trigger.dataset.state = states.join(" ");
      else trigger.removeAttribute("data-state");
    }
    if (open) {
      popover.previousFocus = document.activeElement;
      panel?.focus({ preventScroll: true });
      requestAnimationFrame(() => window.pennaKit?.roundAll());
    } else if (panel?.contains(document.activeElement)) {
      popover.previousFocus?.focus?.({ preventScroll: true });
    }
    popover.dispatchEvent(new CustomEvent(open ? "penna-open" : "penna-dismiss", { bubbles: true }));
  }

  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0) return;
    const trigger = event.target.closest(".penna-popover-trigger");
    document.querySelectorAll(".penna-popover[data-open]").forEach((popover) => {
      if (popover.contains(event.target)) return;
      if (popover.dataset.closable !== "false") setOpen(popover, false);
    });
    if (!trigger) return;
    const popover = trigger.closest(".penna-popover");
    event.preventDefault();
    setOpen(popover, !popover.hasAttribute("data-open"));
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    const popover = event.target.closest?.(".penna-popover[data-open]");
    if (popover) setOpen(popover, false);
  });

  document.querySelectorAll(".penna-popover[data-open]").forEach((popover) => {
    triggerOf(popover)?.setAttribute("aria-expanded", "true");
  });

  window.pennaPopover = { setOpen };
})();
