(() => {
  const SLACK = 1;
  const THRESHOLD = 10;
  const ARROWS = ["up", "left", "right", "down"];

  function edges(row) {
    const scroll = row.querySelector(".penna-key-row-scroll");
    const max = scroll.scrollWidth - scroll.clientWidth;
    const back = scroll.scrollLeft > SLACK;
    const ahead = max > SLACK && scroll.scrollLeft < max - SLACK;
    row.querySelector('[data-side="back"]').toggleAttribute("data-shown", back);
    row.querySelector('[data-side="ahead"]').toggleAttribute("data-shown", ahead);
  }

  function wireRow(row) {
    const scroll = row.querySelector(".penna-key-row-scroll");
    if (row.hasAttribute("data-end")) scroll.scrollLeft = scroll.scrollWidth;
    scroll.addEventListener("scroll", () => edges(row), { passive: true });
    row.querySelector('[data-side="back"]').addEventListener("click", () => scroll.scrollTo({ left: 0 }));
    row.querySelector('[data-side="ahead"]').addEventListener("click", () => scroll.scrollTo({ left: scroll.scrollWidth }));
    edges(row);
  }

  function press(key) {
    key.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      key.setPointerCapture(event.pointerId);
      key.setAttribute("data-pressed", "");
    });
    const release = () => key.removeAttribute("data-pressed");
    key.addEventListener("pointerup", release);
    key.addEventListener("pointercancel", release);
  }

  function bubble() {
    const card = document.createElement("div");
    card.className = "penna-arrow-bubble";
    card.dataset.penna = "popover";
    const cells = ["", "up", "", "left", "hub", "right", "", "down", ""];
    for (const name of cells) {
      const cell = document.createElement("span");
      if (ARROWS.includes(name)) {
        cell.className = "penna-arrow-cell";
        cell.dataset.penna = "phone.arrow-pad";
        cell.dataset.direction = name;
        const icon = document.createElement("i");
        icon.dataset.icon = `arrow-${name}`;
        cell.append(icon);
      } else if (name === "hub") {
        cell.className = "penna-arrow-hub";
      }
      card.append(cell);
    }
    return card;
  }

  function direction(dx, dy) {
    if (Math.hypot(dx, dy) < THRESHOLD) return null;
    if (Math.abs(dx) >= Math.abs(dy)) return dx < 0 ? "left" : "right";
    return dy < 0 ? "up" : "down";
  }

  function wirePad(pad) {
    let origin = null;
    let card = null;
    pad.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      pad.setPointerCapture(event.pointerId);
      origin = [event.clientX, event.clientY];
      pad.setAttribute("data-pressed", "");
      card = bubble();
      pad.append(card);
    });
    pad.addEventListener("pointermove", (event) => {
      if (!origin || !card) return;
      const held = direction(event.clientX - origin[0], event.clientY - origin[1]);
      card.querySelectorAll(".penna-arrow-cell").forEach((cell) => {
        cell.dataset.state = cell.dataset.direction === held ? "selected" : "";
      });
    });
    const release = () => {
      origin = null;
      pad.removeAttribute("data-pressed");
      card?.remove();
      card = null;
    };
    pad.addEventListener("pointerup", release);
    pad.addEventListener("pointercancel", release);
  }

  function settleRows() {
    document.querySelectorAll(".penna-key-row").forEach((row) => {
      const scroll = row.querySelector(".penna-key-row-scroll");
      if (row.hasAttribute("data-end")) scroll.scrollLeft = scroll.scrollWidth;
      edges(row);
    });
  }

  function grounds() {
    const probe = document.createElement("div");
    probe.dataset.penna = "phone.pill";
    probe.hidden = true;
    document.body.append(probe);
    const backdrop = getComputedStyle(probe).getPropertyValue("--penna-backdrop").trim();
    probe.remove();
    const glass = Boolean(backdrop) && backdrop !== "none";
    document.querySelectorAll(".penna-key").forEach((key) => key.toggleAttribute("data-on-glass", glass));
  }

  grounds();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && grounds(), true);
  document.querySelectorAll(".penna-key-row").forEach(wireRow);
  document.fonts.ready.then(() => requestAnimationFrame(settleRows));
  document.querySelectorAll(".penna-key:not(.penna-arrow-pad)").forEach((key) => key.closest(".penna-key-row") || press(key));
  document.querySelectorAll(".penna-arrow-pad").forEach(wirePad);
  addEventListener("resize", () => document.querySelectorAll(".penna-key-row").forEach(edges));
})();
