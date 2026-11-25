(() => {
  function wire(root) {
    root.querySelectorAll(".penna-hover-card").forEach((el) => {
      if (el.dataset.hoverCardWired || el.hasAttribute("data-still")) return;
      el.dataset.hoverCardWired = "1";
      let openTimer = 0;
      let closeTimer = 0;
      const enter = () => {
        clearTimeout(closeTimer);
        if (el.hasAttribute("data-open")) return;
        openTimer = setTimeout(() => {
          el.setAttribute("data-open", "");
          window.pennaKit?.roundAll?.(el);
        }, 200);
      };
      const leave = () => {
        clearTimeout(openTimer);
        closeTimer = setTimeout(() => {
          el.removeAttribute("data-open");
        }, 140);
      };
      el.addEventListener("pointerenter", enter);
      el.addEventListener("pointerleave", leave);
      el.addEventListener("focusin", enter);
      el.addEventListener("focusout", (event) => {
        if (!el.contains(event.relatedTarget)) leave();
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaHoverCard = { wire };
})();
