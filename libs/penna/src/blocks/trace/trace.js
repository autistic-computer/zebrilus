(() => {
  function wire(root) {
    root.querySelectorAll(".penna-trace").forEach((trace) => {
      if (trace.dataset.traceWired) return;
      trace.dataset.traceWired = "1";
      trace.querySelectorAll(".penna-trace-step").forEach((step) => {
        const row = step.querySelector(".penna-trace-row");
        const detail = step.querySelector(".penna-trace-detail");
        if (!row || !detail) return;
        row.addEventListener("click", () => {
          const next = row.getAttribute("aria-expanded") !== "true";
          row.setAttribute("aria-expanded", String(next));
          detail.hidden = !next;
        });
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaTrace = { wire };
})();
