(() => {
  const ROWS = ".penna-list-item[data-penna]:not([data-disabled], [aria-disabled='true'])";

  function hoverFill(row) {
    const saved = row.getAttribute("data-state");
    row.setAttribute("data-state", "hover");
    const fill = getComputedStyle(row).getPropertyValue("--penna-fill").trim();
    if (saved === null) row.removeAttribute("data-state");
    else row.setAttribute("data-state", saved);
    return fill;
  }

  function settle() {
    document.querySelectorAll(ROWS).forEach((row) => {
      row.style.setProperty("--list-hover-fill", hoverFill(row) || "transparent");
      row.setAttribute("data-press", "hover");
    });
  }

  settle();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(settle), true);
})();
