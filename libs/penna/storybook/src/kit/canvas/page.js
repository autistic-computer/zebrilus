(() => {
  const SLOP = 3;

  function states(tile) {
    return new Set((tile.dataset.state || "").split(/\s+/).filter(Boolean));
  }

  function setStates(tile, set) {
    if (set.size) tile.dataset.state = [...set].join(" ");
    else tile.removeAttribute("data-state");
  }

  function pick(tile) {
    const group = tile.closest("[data-pick]");
    if (!group || states(tile).has("disabled")) return;
    group.querySelectorAll(".penna-tile").forEach((other) => {
      const set = states(other);
      set.delete("selected");
      if (other === tile) set.add("selected");
      setStates(other, set);
    });
    group.dispatchEvent(new CustomEvent("penna-pick", { bubbles: true, detail: { id: tile.dataset.id } }));
  }

  function draggable(tile) {
    let dropped = false;
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
        if (!moved) return;
        dropped = true;
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
    tile.addEventListener("click", (event) => {
      if (!dropped) return;
      dropped = false;
      event.stopImmediatePropagation();
    });
  }

  document.querySelectorAll("[data-tiles] > .penna-tile").forEach(draggable);
  document.querySelectorAll("[data-pick] > .penna-tile").forEach((tile) => {
    tile.addEventListener("click", () => pick(tile));
  });
})();
