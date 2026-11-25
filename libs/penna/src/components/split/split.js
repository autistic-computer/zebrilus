(() => {
  function wire(root) {
    root.querySelectorAll(".penna-split-panes").forEach((el) => {
      if (el.dataset.splitWired) return;
      el.dataset.splitWired = "1";
      const sash = el.querySelector(".penna-split-sash");
      if (!sash) return;
      const vertical = el.dataset.axis === "vertical";
      const min = Number(el.dataset.min || 18);
      const max = Number(el.dataset.max || 82);
      const initial = Number(el.dataset.ratio || 50);
      let current = initial;

      function apply(pct) {
        current = Math.min(max, Math.max(min, pct));
        el.style.setProperty("--penna-split", `${current.toFixed(1)}%`);
        sash.setAttribute("aria-valuenow", String(Math.round(current)));
        el.dispatchEvent(new CustomEvent("resize", { bubbles: true, detail: { ratio: current / 100 } }));
      }

      apply(initial);

      sash.addEventListener("dblclick", () => apply(initial));

      sash.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        sash.setPointerCapture(event.pointerId);
        el.setAttribute("data-dragging", "");
        const onMove = (moveEvent) => {
          const rect = el.getBoundingClientRect();
          const span = vertical ? rect.height : rect.width;
          if (span <= 1) return;
          const pos = vertical ? moveEvent.clientY - rect.top : moveEvent.clientX - rect.left;
          apply((pos / span) * 100);
        };
        const onUp = () => {
          el.removeAttribute("data-dragging");
          sash.removeEventListener("pointermove", onMove);
          sash.removeEventListener("pointerup", onUp);
          sash.removeEventListener("pointercancel", onUp);
        };
        sash.addEventListener("pointermove", onMove);
        sash.addEventListener("pointerup", onUp);
        sash.addEventListener("pointercancel", onUp);
      });

      sash.addEventListener("keydown", (event) => {
        const step = event.shiftKey ? 10 : 2;
        if ((!vertical && event.key === "ArrowLeft") || (vertical && event.key === "ArrowUp")) {
          event.preventDefault();
          apply(current - step);
        } else if ((!vertical && event.key === "ArrowRight") || (vertical && event.key === "ArrowDown")) {
          event.preventDefault();
          apply(current + step);
        } else if (event.key === "Home") {
          event.preventDefault();
          apply(min);
        } else if (event.key === "End") {
          event.preventDefault();
          apply(max);
        }
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaSplit = { wire };
})();
