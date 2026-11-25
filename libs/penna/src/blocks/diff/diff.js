(() => {
  function wire(root) {
    root.querySelectorAll(".penna-diff").forEach((diff) => {
      if (diff.dataset.diffWired) return;
      diff.dataset.diffWired = "1";
      const seg = diff.querySelector("[data-diff-layout]");
      seg?.addEventListener("change", (event) => {
        diff.dataset.layout = event.detail.index === 1 ? "split" : "unified";
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaDiff = { wire };
})();
