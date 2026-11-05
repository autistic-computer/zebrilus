(() => {
  const MARGIN = 8;

  function panelOf(popover) {
    popover.panel ??= popover.querySelector(":scope > .penna-popover-panel");
    return popover.panel;
  }

  function triggerOf(popover) {
    return popover.querySelector(":scope > .penna-popover-trigger");
  }

  function inside(rect, view) {
    return rect.bottom > view.top && rect.top < view.bottom && rect.right > view.left && rect.left < view.right;
  }

  function fit(popover) {
    const panel = panelOf(popover);
    const trigger = triggerOf(popover);
    if (!panel?.isConnected) return;
    panel.style.removeProperty("translate");
    const visual = window.visualViewport;
    const left = visual?.offsetLeft ?? 0;
    const top = visual?.offsetTop ?? 0;
    const view = { left, top, right: left + (visual?.width ?? innerWidth), bottom: top + (visual?.height ?? innerHeight) };
    if (trigger && !inside(trigger.getBoundingClientRect(), view)) return;
    const rect = panel.getBoundingClientRect();
    const x = Math.max(Math.min(rect.left, view.right - MARGIN - rect.width), view.left + MARGIN);
    const y = Math.max(Math.min(rect.top, view.bottom - MARGIN - rect.height), view.top + MARGIN);
    if (x !== rect.left || y !== rect.top) panel.style.translate = `${x - rect.left}px ${y - rect.top}px`;
  }

  function fitAll() {
    document.querySelectorAll(".penna-popover[data-open]").forEach(fit);
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
    if (open && panel && !panel.isConnected) popover.append(panel);
    if (open) {
      fit(popover);
      popover.previousFocus = document.activeElement;
      panel?.focus({ preventScroll: true });
      requestAnimationFrame(() => window.pennaKit?.roundAll());
    } else if (panel?.contains(document.activeElement)) {
      popover.previousFocus?.focus?.({ preventScroll: true });
    }
    if (!open) panel?.remove();
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

  document.addEventListener("keydown", (event) => {
    if (!["Enter", " ", "ArrowDown"].includes(event.key) || event.repeat || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const trigger = event.target.closest?.(".penna-popover-trigger");
    if (!trigger) return;
    event.preventDefault();
    const popover = trigger.closest(".penna-popover");
    setOpen(popover, !popover.hasAttribute("data-open"));
  });

  document.querySelectorAll(".penna-popover:not([data-open])").forEach((popover) => {
    panelOf(popover)?.remove();
  });

  document.querySelectorAll(".penna-popover[data-open]").forEach((popover) => {
    triggerOf(popover)?.setAttribute("aria-expanded", "true");
    fit(popover);
    popover.previousFocus = document.activeElement;
    panelOf(popover)?.focus({ preventScroll: true });
  });

  window.visualViewport?.addEventListener("resize", fitAll);
  window.visualViewport?.addEventListener("scroll", fitAll);
  window.addEventListener("resize", fitAll);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && fitAll(), true);

  window.pennaPopover = { setOpen };
})();
