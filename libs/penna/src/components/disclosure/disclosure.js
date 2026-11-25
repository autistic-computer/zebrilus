(() => {
  function wire(root) {
    root.querySelectorAll(".penna-disclosure").forEach((el) => {
      if (el.dataset.disclosureWired) return;
      el.dataset.disclosureWired = "1";
      const trigger = el.querySelector(".penna-disclosure-trigger");
      const body = el.querySelector(".penna-disclosure-body");
      if (!trigger || !body) return;
      trigger.addEventListener("click", () => {
        const next = trigger.getAttribute("aria-expanded") !== "true";
        trigger.setAttribute("aria-expanded", String(next));
        body.hidden = !next;
        el.dispatchEvent(new CustomEvent("change", { bubbles: true, detail: { open: next } }));
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaDisclosure = { wire };
})();
