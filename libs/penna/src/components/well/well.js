(() => {
  function luma(value) {
    const probe = document.createElement("i");
    probe.style.color = value;
    document.body.append(probe);
    const channels = getComputedStyle(probe).color.match(/[\d.]+/g)?.map(Number) ?? [0, 0, 0];
    probe.remove();
    return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
  }

  function bright() {
    const root = getComputedStyle(document.documentElement);
    const ground = luma(root.getPropertyValue("--penna-bg").trim());
    const light = document.documentElement.dataset.pennaMode === "light";
    document.querySelectorAll(".penna-well").forEach((well) => {
      const tile = well.querySelector(".penna-tile");
      const fill = tile ? getComputedStyle(tile).getPropertyValue("--penna-fill").trim() : "";
      well.toggleAttribute("data-bright", light && Boolean(fill) && luma(fill) > ground);
    });
  }

  bright();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && bright(), true);
})();
