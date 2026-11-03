(() => {
  function wire(example) {
    const tabs = example.querySelector(".penna-docs-tabs .penna-segmented");
    const panes = [...example.querySelectorAll(".penna-docs-code > [data-tab]")];
    if (tabs) {
      tabs.addEventListener("change", (event) => {
        panes.forEach((pane, index) => (pane.hidden = index !== event.detail.index));
      });
    }
    example.querySelectorAll("[data-copy]").forEach((button) => {
      button.addEventListener("click", () => {
        const code = button.closest("pre")?.querySelector("code")?.textContent ?? "";
        navigator.clipboard?.writeText(code).catch(() => {});
      });
    });
  }

  document.querySelectorAll(".penna-docs-example").forEach(wire);
})();
