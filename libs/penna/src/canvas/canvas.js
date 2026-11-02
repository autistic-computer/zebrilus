(() => {
  const TEXT = ".penna-type";
  const SLOP = 3;

  function snapText(root = document) {
    const runs = [...root.querySelectorAll(TEXT)];
    runs.forEach((run) => run.style.removeProperty("width"));
    const widths = runs.map((run) => run.getBoundingClientRect().width);
    runs.forEach((run, index) => {
      if (run.scrollWidth > run.clientWidth + 0.5) return;
      run.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  function states(tile) {
    return new Set((tile.dataset.state || "").split(/\s+/).filter(Boolean));
  }

  function setStates(tile, set) {
    if (set.size) tile.dataset.state = [...set].join(" ");
    else tile.removeAttribute("data-state");
  }

  function pick(tile) {
    const group = tile.closest("[data-pick]");
    if (!group) return;
    group.querySelectorAll(".penna-tile").forEach((other) => {
      const set = states(other);
      set.delete("selected");
      if (other === tile) set.add("selected");
      setStates(other, set);
    });
    group.dispatchEvent(new CustomEvent("penna-pick", { bubbles: true, detail: { id: tile.dataset.id } }));
  }

  function draggable(tile) {
    tile.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || states(tile).has("disabled")) return;
      const start = { x: event.clientX, y: event.clientY };
      const before = tile.getAttribute("data-state");
      let moved = false;
      tile.setPointerCapture(event.pointerId);
      const move = (next) => {
        const dx = next.clientX - start.x;
        const dy = next.clientY - start.y;
        if (!moved && Math.hypot(dx, dy) < SLOP) return;
        if (!moved) {
          moved = true;
          const set = states(tile);
          set.add("hover");
          setStates(tile, set);
          tile.setAttribute("data-dragging", "");
        }
        tile.style.translate = `${dx}px ${dy}px`;
      };
      const end = (last) => {
        tile.removeEventListener("pointermove", move);
        tile.removeEventListener("pointerup", end);
        tile.removeEventListener("pointercancel", end);
        if (!moved) {
          pick(tile);
          return;
        }
        tile.style.removeProperty("translate");
        tile.removeAttribute("data-dragging");
        if (before === null) tile.removeAttribute("data-state");
        else tile.setAttribute("data-state", before);
        tile.dispatchEvent(new CustomEvent("penna-drop", { bubbles: true, detail: { id: tile.dataset.id, x: last.clientX, y: last.clientY } }));
      };
      tile.addEventListener("pointermove", move);
      tile.addEventListener("pointerup", end);
      tile.addEventListener("pointercancel", end);
    });
  }

  document.querySelectorAll("[data-tiles] > .penna-tile").forEach(draggable);

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

  function glass() {
    document.querySelectorAll(".penna-tile").forEach((tile) => {
      tile.style.removeProperty("backdrop-filter");
      tile.removeAttribute("data-glass");
      const backdrop = getComputedStyle(tile).getPropertyValue("--penna-backdrop").trim();
      const glassy = backdrop !== "" && backdrop !== "none";
      tile.toggleAttribute("data-glass", glassy);
      if (glassy && tile.dataset.variant !== "veil") {
        tile.style.backdropFilter = backdrop.replace(/contrast\([^)]*\)/, "").trim();
      }
    });
  }

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

  function themed() {
    glass();
    bright();
  }

  themed();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && themed(), true);
  snapText();
  document.fonts.ready.then(() => snapText());
})();
