(() => {
  function insetRadius(surface) {
    surface.style.removeProperty("--penna-r");
    const inset = parseFloat(surface.dataset.inset ?? "0") || 0;
    const radius = parseFloat(getComputedStyle(surface).getPropertyValue("--penna-r")) || 0;
    if (radius > 0 && radius < 9999 && inset > 0) {
      const root = getComputedStyle(document.documentElement);
      const rows = parseFloat(root.getPropertyValue("--penna-control-md")) || 28;
      const roundness = parseFloat(root.getPropertyValue("--penna-control-share")) || 0;
      surface.style.setProperty("--penna-r", `${roundness * rows + inset}px`);
    }
  }

  function settlePalettes() {
    document.querySelectorAll(".penna-palette-surface[data-inset]").forEach(insetRadius);
    globalThis.pennaKit?.roundAll();
  }

  function wireToggles(root = document) {
    root.querySelectorAll(".penna-palette-disclosure[data-toggle]").forEach((disclosure) => {
      if (disclosure.pennaPalette) return;
      disclosure.pennaPalette = true;
      disclosure.addEventListener("mousedown", (event) => event.preventDefault());
      disclosure.addEventListener("click", (event) => {
        event.stopPropagation();
        const row = disclosure.closest(".penna-palette-row");
        const open = row?.getAttribute("aria-expanded") !== "true";
        row?.setAttribute("aria-expanded", String(open));
        disclosure.querySelector("i")?.setAttribute("data-icon", open ? "chevron-down" : "chevron-right");
        row?.dispatchEvent(new CustomEvent("penna-toggle", { bubbles: true, detail: { open } }));
      });
    });
  }

  function moveSelection(list, step) {
    const rows = [...list.querySelectorAll(".penna-palette-row")];
    if (!rows.length) return;
    const at = rows.findIndex((row) => row.getAttribute("aria-selected") === "true");
    const next = Math.min(Math.max((at < 0 ? -1 : at) + step, 0), rows.length - 1);
    rows.forEach((row, index) => {
      const selected = index === next;
      row.setAttribute("aria-selected", String(selected));
      row.toggleAttribute("data-state", false);
      if (selected) row.setAttribute("data-state", "selected");
    });
    rows[next].scrollIntoView({ block: "nearest" });
  }

  document.addEventListener("keydown", (event) => {
    const surface = event.target.closest?.(".penna-palette-surface");
    const list = surface?.querySelector(".penna-palette-rows");
    if (!list) return;
    if (event.key === "ArrowDown") moveSelection(list, 1);
    else if (event.key === "ArrowUp") moveSelection(list, -1);
    else return;
    event.preventDefault();
  });

  settlePalettes();
  wireToggles();
  document.fonts?.ready.then(settlePalettes);
  document.addEventListener("penna-theme", () => requestAnimationFrame(settlePalettes));
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settlePalettes(), true);
})();
