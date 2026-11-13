(() => {
  const SLACK = 1;
  const THRESHOLD = 10;
  const MARGIN = 8;
  const EDGE = 16;
  const GAP = 8;
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
    card.dataset.penna = "floating";
    card.dataset.size = "touch";
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

  function snap(card, frame) {
    const bounds = frame.getBoundingClientRect();
    const box = card.getBoundingClientRect();
    const shift = (start, end, low, high) => Math.max(low + MARGIN - start, Math.min(0, high - MARGIN - end));
    card.style.translate = `${shift(box.left, box.right, bounds.left, bounds.right)}px ${shift(box.top, box.bottom, bounds.top, bounds.bottom)}px`;
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
      snap(card, pad.closest("section") ?? document.documentElement);
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

  function over(fill) {
    const paint = document.createElement("canvas").getContext("2d");
    paint.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--penna-bg").trim() || "#fff";
    paint.fillRect(0, 0, 1, 1);
    paint.fillStyle = fill;
    paint.fillRect(0, 0, 1, 1);
    const [r, g, b] = paint.getImageData(0, 0, 1, 1).data;
    return `rgb(${r} ${g} ${b})`;
  }

  function grounds() {
    const probe = document.createElement("div");
    probe.dataset.penna = "phone.pill";
    probe.hidden = true;
    document.body.append(probe);
    const style = getComputedStyle(probe);
    const backdrop = style.getPropertyValue("--penna-backdrop").trim();
    const fill = style.getPropertyValue("--penna-fill").trim() || "transparent";
    probe.remove();
    const glass = Boolean(backdrop) && backdrop !== "none";
    const shade = `rgb(from ${fill} r g b / 1)`;
    document.querySelectorAll(".penna-key, .penna-key-row, .penna-compact-pill").forEach((element) => element.toggleAttribute("data-on-glass", glass));
    document.querySelectorAll(".penna-key-row").forEach((row) => row.style.setProperty("--penna-more-shade", glass ? shade : over(fill)));
  }

  function still() {
    document.querySelectorAll(".penna-key, .penna-compact-pill").forEach((element) => {
      const probe = document.createElement(element.tagName);
      for (const name of ["data-penna", "data-tone", "data-prominent"]) {
        if (element.hasAttribute(name)) probe.setAttribute(name, element.getAttribute(name));
      }
      probe.hidden = true;
      document.body.append(probe);
      element.style.setProperty("--penna-rest-fill", getComputedStyle(probe).getPropertyValue("--penna-fill").trim() || "transparent");
      probe.remove();
    });
  }

  function placeStack(stack) {
    if (!stack) return;
    const anchor = stack.parentElement.querySelector(stack.dataset.from);
    const column = stack.querySelector(".penna-stack-column");
    if (!anchor || !column) return;
    const frame = stack.getBoundingClientRect();
    const at = anchor.getBoundingClientRect();
    column.removeAttribute("data-align");
    column.style.left = "0px";
    const width = column.offsetWidth;
    let left = at.left - frame.left + at.width / 2 - width / 2;
    if (left < EDGE) {
      left = EDGE;
      column.dataset.align = "start";
    } else if (left + width > frame.width - EDGE) {
      left = Math.max(EDGE, frame.width - EDGE - width);
      column.dataset.align = "end";
    }
    column.style.left = `${left}px`;
    column.style.top = `${at.top - frame.top - GAP - column.offsetHeight}px`;
    const close = stack.querySelector(".penna-stack-close");
    if (close) {
      close.style.left = `${at.left - frame.left + at.width / 2 - close.offsetWidth / 2}px`;
      close.style.top = `${at.top - frame.top + at.height / 2 - close.offsetHeight / 2}px`;
    }
  }

  function stacks() {
    document.querySelectorAll(".penna-stack[data-from]").forEach(placeStack);
  }

  const resized = new ResizeObserver((entries) => entries.forEach((entry) => placeStack(entry.target.closest(".penna-stack"))));
  document.querySelectorAll(".penna-stack[data-from] > .penna-stack-column").forEach((column) => resized.observe(column));

  grounds();
  still();
  stacks();
  document.fonts.ready.then(stacks);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && (grounds(), still(), stacks()), true);
  document.querySelectorAll(".penna-key-row").forEach(wireRow);
  document.fonts.ready.then(() => requestAnimationFrame(settleRows));
  document.querySelectorAll(".penna-key:not(.penna-arrow-pad)").forEach((key) => key.closest(".penna-key-row") || press(key));
  document.querySelectorAll(".penna-arrow-pad").forEach(wirePad);
  addEventListener("resize", () => document.querySelectorAll(".penna-key-row").forEach(edges));
})();
