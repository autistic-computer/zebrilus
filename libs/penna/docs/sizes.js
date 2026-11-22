(() => {
  if (!document.querySelector("[data-docs-sizes]")) return;

  const stage = document.querySelector("[data-sizes-density-stage]");
  const control = document.querySelector("[data-sizes-density]");
  const readout = document.querySelector("[data-sizes-density-val]");

  const DENSITIES = [
    { px: "24px", label: "Compact · 24 px" },
    { px: "28px", label: "Regular · 28 px" },
    { px: "32px", label: "Comfortable · 32 px" },
  ];

  function applyDensity(index) {
    const entry = DENSITIES[index] ?? DENSITIES[1];
    if (stage) {
      stage.style.setProperty("--penna-control-md", entry.px);
      requestAnimationFrame(() => {
        window.pennaKit?.ceilText?.(stage);
        window.pennaKit?.roundAll?.(stage);
      });
    }
    if (readout) readout.textContent = entry.label;
  }

  control?.addEventListener("change", (event) => applyDensity(event.detail.index));

  const rows = [...document.querySelectorAll("[data-sizes-row]")];
  rows.forEach((row) => {
    const pick = () => {
      rows.forEach((other) => {
        const on = other === row;
        other.setAttribute("aria-selected", String(on));
        if (on) other.dataset.state = "selected";
        else other.removeAttribute("data-state");
      });
    };
    row.addEventListener("click", pick);
    row.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      pick();
    });
  });

  document.querySelectorAll("[data-copy-token]").forEach((btn) => {
    if (btn.dataset.copyWired) return;
    btn.dataset.copyWired = "1";
    let timer = null;
    const original = btn.textContent;
    btn.addEventListener("click", () => {
      const value = btn.dataset.copyToken;
      if (!value) return;
      navigator.clipboard?.writeText(value).catch(() => {});
      clearTimeout(timer);
      btn.dataset.copied = "";
      btn.textContent = "Copied";
      timer = setTimeout(() => {
        delete btn.dataset.copied;
        btn.textContent = original;
      }, 1200);
    });
  });

  document.addEventListener("penna-theme", () => {
    if (stage) requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
  });

  applyDensity(1);
})();
