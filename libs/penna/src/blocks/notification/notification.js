(() => {
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

  window.pennaToast = { dismiss };
})();
