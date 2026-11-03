(() => {
  document.querySelectorAll("[data-close]").forEach((button) => {
    button.addEventListener("click", () => {
      button.dispatchEvent(new CustomEvent("penna-close", { bubbles: true }));
    });
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    const inspector = document.activeElement?.closest?.(".penna-inspector");
    inspector?.querySelector("[data-close]")?.click();
  });

})();
