(() => {
  function dismiss(toast) {
    if (!toast || toast.hasAttribute("data-still") || toast.hasAttribute("data-closing")) return;
    toast.setAttribute("data-closing", "");
    setTimeout(() => toast.remove(), 150);
  }

  function wireAutohide(toast) {
    if (!toast || toast.hasAttribute("data-still") || toast.dataset.toastTimed) return;
    const ms = Number(toast.dataset.autohide);
    if (!Number.isFinite(ms) || ms <= 0) return;
    toast.dataset.toastTimed = "1";
    let remaining = ms;
    let paused = false;
    let last = performance.now();

    toast.addEventListener("pointerenter", () => {
      paused = true;
    });
    toast.addEventListener("pointerleave", () => {
      paused = false;
      last = performance.now();
    });

    const interval = setInterval(() => {
      if (!toast.isConnected || toast.hasAttribute("data-closing")) {
        clearInterval(interval);
        return;
      }
      const now = performance.now();
      const expanded = toast.querySelector(".penna-toast-message[data-expanded]");
      if (!paused && !document.hidden && !expanded) {
        remaining -= now - last;
        if (remaining <= 0) {
          clearInterval(interval);
          dismiss(toast);
        }
      }
      last = now;
    }, 100);
  }

  document.addEventListener("click", (event) => {
    const close = event.target.closest(".penna-toast-close > button");
    if (close) {
      event.stopPropagation();
      dismiss(close.closest(".penna-toast"));
      return;
    }
    const message = event.target.closest(".penna-toast-message");
    if (message && !event.target.closest("button, a")) {
      message.toggleAttribute("data-expanded");
    }
  });

  document.addEventListener("auxclick", (event) => {
    if (event.button !== 1) return;
    const toast = event.target.closest(".penna-toast");
    if (toast) dismiss(toast);
  });

  document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll(".penna-toast[data-autohide]").forEach(wireAutohide);
  });

  window.pennaToast = { dismiss, wireAutohide };
})();
