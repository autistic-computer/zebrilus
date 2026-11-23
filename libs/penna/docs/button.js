(() => {
  if (!document.querySelector("[data-docs-button]")) return;

  const KINDS = [
    { penna: "button", tone: "", prominent: false },
    { penna: "button", tone: "primary", prominent: true },
    { penna: "button.quiet", tone: "", prominent: false },
    { penna: "button.outline", tone: "", prominent: false },
    { penna: "button", tone: "danger", prominent: false },
  ];

  document.querySelectorAll("[data-btn-demo-link]").forEach((link) => {
    link.addEventListener("click", (event) => event.preventDefault());
  });

  const statesRoot = document.querySelector("[data-btn-states]");
  if (statesRoot) {
    const picker = statesRoot.querySelector("[data-btn-kind-picker]");
    const items = [...statesRoot.querySelectorAll("[data-btn-state-item]")];
    const applyKind = (index) => {
      const spec = KINDS[index] || KINDS[0];
      items.forEach((btn) => {
        btn.dataset.penna = spec.penna;
        if (spec.tone) btn.dataset.tone = spec.tone;
        else delete btn.dataset.tone;
        btn.toggleAttribute("data-prominent", spec.prominent);
      });
      requestAnimationFrame(() => {
        window.pennaKit?.roundAll(statesRoot);
      });
    };
    picker?.addEventListener("change", (event) => applyKind(event.detail.index));
    applyKind(0);
  }

  const busyDemo = document.querySelector("[data-btn-try-busy]");
  if (busyDemo) {
    let busyTimer = 0;
    busyDemo.addEventListener("click", () => {
      if (busyDemo.hasAttribute("data-loading")) return;
      const label = busyDemo.querySelector(".penna-label");
      const spinner = document.createElement("span");
      spinner.className = "penna-spinner";
      spinner.innerHTML = '<i data-icon="loader"></i>';
      busyDemo.setAttribute("data-loading", "");
      busyDemo.prepend(spinner);
      if (label) label.textContent = "Saving";
      window.pennaKit?.ceilText(busyDemo);
      clearTimeout(busyTimer);
      busyTimer = setTimeout(() => {
        busyDemo.removeAttribute("data-loading");
        spinner.remove();
        if (label) label.textContent = "Save";
        window.pennaKit?.ceilText(busyDemo);
      }, 1400);
    });
  }

  const phoneCorners = document.querySelector("[data-btn-phone-corners]");
  if (phoneCorners) {
    const picker = phoneCorners.querySelector("[data-btn-phone-picker]");
    const stage = phoneCorners.querySelector("[data-btn-phone-stage]");
    const barMeta = phoneCorners.querySelector("[data-btn-bar-meta]");
    const discMeta = phoneCorners.querySelector("[data-btn-disc-meta]");
    const sync = (index) => {
      const pill = index === 1;
      if (stage) {
        stage.dataset.look = pill ? "pill" : "default";
        requestAnimationFrame(() => window.pennaKit?.roundAll(stage));
      }
      if (barMeta) barMeta.textContent = pill ? "44 × 44 · circle" : "44 × 44 · 13.7 px";
      if (discMeta) discMeta.textContent = pill ? "56 disc · circle" : "56 disc · 17.5 px";
    };
    picker?.addEventListener("change", (event) => sync(event.detail.index));
    sync(0);
  }
})();
