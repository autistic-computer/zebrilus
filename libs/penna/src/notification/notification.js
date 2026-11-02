(() => {
  const TEXT = ".penna-toast-text > div > span";

  function ceil(root = document) {
    const spans = [...root.querySelectorAll(TEXT)];
    spans.forEach((span) => span.style.removeProperty("width"));
    const widths = spans.map((span) => span.getBoundingClientRect().width);
    spans.forEach((span, index) => {
      span.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  function dismiss(toast) {
    if (toast.hasAttribute("data-closing")) return;
    toast.setAttribute("data-closing", "");
    setTimeout(() => toast.remove(), 150);
  }

  document.addEventListener("click", (event) => {
    const close = event.target.closest(".penna-toast-close > button");
    if (!close) return;
    event.stopPropagation();
    dismiss(close.closest(".penna-toast"));
  });

  document.addEventListener("auxclick", (event) => {
    if (event.button !== 1) return;
    const toast = event.target.closest(".penna-toast");
    if (toast) dismiss(toast);
  });

  ceil();
  document.fonts.ready.then(() => ceil());
  window.pennaToast = { dismiss, ceil };
})();
