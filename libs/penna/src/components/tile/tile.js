(() => {
  const focusable = (event) => event.target.closest?.(".penna-tile[tabindex]");
  const disabled = (tile) => (tile.dataset.state || "").split(/\s+/).includes("disabled");

  document.addEventListener("mousedown", (event) => focusable(event) && event.preventDefault());
  document.addEventListener("keydown", (event) => {
    const tile = focusable(event);
    if (!tile || disabled(tile) || (event.key !== "Enter" && event.key !== " ")) return;
    event.preventDefault();
    tile.click();
  });

  function glass() {
    document.querySelectorAll(".penna-tile").forEach((tile) => {
      tile.toggleAttribute("data-glass", getComputedStyle(tile).getPropertyValue("--penna-glass").trim() === "1");
    });
  }

  glass();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && glass(), true);
})();
