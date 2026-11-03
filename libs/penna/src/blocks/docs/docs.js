(() => {
  function wire(example) {
    const tabs = example.querySelector(".penna-docs-tabs .penna-segmented");
    const panes = [...example.querySelectorAll(".penna-docs-code > [data-tab]")];
    if (tabs) {
      tabs.addEventListener("change", (event) => {
        panes.forEach((pane, index) => (pane.hidden = index !== event.detail.index));
      });
    }
  }

  document.querySelectorAll(".penna-docs-example").forEach(wire);

  document.querySelectorAll("[data-docs-menu]").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelector(".penna-docs-side")?.toggleAttribute("data-open");
    });
  });

  document.querySelectorAll("[data-docs-mode]").forEach((button) => {
    button.addEventListener("click", () => {
      pennaKit.choice.mode = pennaKit.choice.mode === "dark" ? "light" : "dark";
      pennaKit.apply();
    });
  });
})();
