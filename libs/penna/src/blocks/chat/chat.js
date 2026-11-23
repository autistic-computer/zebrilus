(() => {
  const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)");

  function paintResolve(label, text) {
    const shown = [...`${text}…`];
    label.style.removeProperty("width");
    if (reduced?.matches) {
      label.textContent = shown.join("");
      return;
    }
    label.style.setProperty("--penna-chat-stagger", `${Math.min(18, 400 / shown.length)}ms`);
    label.replaceChildren(
      ...shown.map((character, index) => {
        const glyph = document.createElement("i");
        glyph.style.setProperty("--i", index);
        glyph.textContent = character;
        return glyph;
      }),
    );
  }

  document.querySelectorAll(".penna-chat-resolve[data-resolve-labels]").forEach((label) => {
    const items = label.dataset.resolveLabels.split("|").map((item) => item.trim()).filter(Boolean);
    if (!items.length) return;
    let index = 0;
    paintResolve(label, items[index]);
    if (items.length > 1) {
      setInterval(() => {
        index = (index + 1) % items.length;
        paintResolve(label, items[index]);
      }, 2400);
    }
  });

  document.querySelectorAll(".penna-chat-tray[data-interactive]").forEach((tray) => {
    const chip = tray.querySelector(".penna-chat-plan-chip");
    const panel = tray.querySelector(".penna-chat-tray-panel");
    const chevron = chip?.querySelector(".penna-chat-glyph:last-child i");
    if (!chip || !panel) return;
    const sync = (open) => {
      panel.hidden = !open;
      chip.setAttribute("aria-expanded", String(open));
      if (open) chip.dataset.state = "current";
      else chip.removeAttribute("data-state");
      chevron?.setAttribute("data-icon", open ? "chevron-down" : "chevron-up");
    };
    sync(!panel.hidden);
    chip.addEventListener("click", () => sync(panel.hidden));
  });

  document.querySelectorAll(".penna-chat-ask[data-interactive]").forEach((ask) => {
    const options = [...ask.querySelectorAll(".penna-chat-ask-option:not([data-state='disabled'])")];
    const multi = options.some((option) => option.getAttribute("role") === "checkbox");
    const counter = ask.closest(".penna-chat-frame")?.querySelector("[data-picked-count]");
    const syncCount = () => {
      if (!counter) return;
      const count = options.filter((option) => option.getAttribute("aria-checked") === "true").length;
      counter.textContent = `${count} picked`;
    };
    options.forEach((option) => {
      option.addEventListener("click", () => {
        if (multi) {
          const next = option.getAttribute("aria-checked") !== "true";
          option.setAttribute("aria-checked", String(next));
          const cap = option.querySelector(".penna-kbd");
          if (cap) {
            if (next) {
              cap.dataset.tone = "accent";
              cap.dataset.emphasis = "solid";
              cap.dataset.solid = "accent";
            } else {
              delete cap.dataset.tone;
              delete cap.dataset.emphasis;
              delete cap.dataset.solid;
            }
          }
          options.forEach((item) => {
            const on = item === option;
            if (on) item.dataset.state = "selected";
            else item.removeAttribute("data-state");
          });
          syncCount();
        } else {
          options.forEach((item) => {
            const on = item === option;
            item.setAttribute("aria-selected", String(on));
            if (on) item.dataset.state = "selected";
            else item.removeAttribute("data-state");
          });
        }
      });
    });
  });
})();
