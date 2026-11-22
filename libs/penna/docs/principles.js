(() => {
  const root = document.querySelector("[data-docs-principles]");
  if (!root) return;

  document.querySelectorAll("[data-principles-menu]").forEach((host) => {
    const list = host.querySelector('[role="menu"]');
    const rows = [...host.querySelectorAll('[role="menuitem"]')];
    const status = host.querySelector("[data-principles-menu-status]");
    const key = host.querySelector("[data-principles-menu-key]");
    if (!list || !rows.length) return;

    let current = rows.findIndex((row) => row.hasAttribute("data-selected"));
    if (current < 0) current = 0;

    const paint = (index) => {
      current = index;
      rows.forEach((row, i) => {
        const on = i === current;
        row.toggleAttribute("data-selected", on);
        if (on) row.dataset.state = "selected";
        else row.removeAttribute("data-state");
      });
    };

    const show = (label, stroke) => {
      if (status) status.textContent = label;
      if (key) {
        if (stroke) {
          key.dataset.kbd = stroke;
          key.hidden = false;
          window.pennaKbd?.fill?.(host);
        } else {
          key.hidden = true;
        }
      }
      window.pennaKit?.ceilText?.(host);
    };

    const run = (index) => {
      const row = rows[index];
      if (!row) return;
      paint(index);
      show(`Ran ${row.dataset.action}`, row.dataset.shortcut || "");
    };

    rows.forEach((row, index) => {
      row.addEventListener("mouseenter", () => paint(index));
      row.addEventListener("click", () => run(index));
    });

    list.addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        paint(current < 0 ? 0 : (current + 1) % rows.length);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        paint(current < 0 ? rows.length - 1 : (current - 1 + rows.length) % rows.length);
      } else if (event.key === "Home") {
        event.preventDefault();
        paint(0);
      } else if (event.key === "End") {
        event.preventDefault();
        paint(rows.length - 1);
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        if (current >= 0) run(current);
      } else if (event.key === "Escape") {
        event.preventDefault();
        paint(-1);
        show("Focus the menu and step with arrow keys.", "");
      }
    });

    paint(current);
  });

  document.querySelectorAll("[data-principles-list]").forEach((list) => {
    const rows = [...list.querySelectorAll("[data-principles-row]")];
    const pick = (chosen) => {
      rows.forEach((row) => {
        const on = row === chosen;
        row.setAttribute("aria-selected", String(on));
        if (on) row.dataset.state = "selected";
        else row.removeAttribute("data-state");
      });
    };
    rows.forEach((row) => {
      row.addEventListener("click", () => pick(row));
      row.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        pick(row);
      });
    });
  });

  document.querySelectorAll("[data-principles-field]").forEach((field) => {
    const wrap = field.querySelector(".penna-input");
    const input = field.querySelector("input");
    const note = field.querySelector("[data-principles-field-note]");
    if (!wrap || !input || !note) return;

    const sync = () => {
      const valid = /^[a-zA-Z0-9-]+$/.test(input.value.trim());
      if (valid) {
        wrap.removeAttribute("data-tone");
        note.className = "penna-field-help";
        note.innerHTML = '<span data-penna-text>Host name is valid.</span>';
      } else {
        wrap.dataset.tone = "danger";
        note.className = "penna-field-error";
        note.innerHTML = '<i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span>';
      }
      window.pennaKit?.ceilText?.(field);
      window.pennaKit?.roundAll?.(field);
    };

    input.addEventListener("input", sync);
  });

  document.querySelectorAll("[data-principles-save]").forEach((card) => {
    const button = card.querySelector("button");
    const badge = card.querySelector("[data-principles-save-badge]");
    if (!button || !badge) return;
    let timer = null;

    const renderButton = (loading, text) => {
      button.toggleAttribute("data-loading", loading);
      button.innerHTML = loading
        ? `<span class="penna-spinner"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>${text}</span>`
        : `<span class="penna-label" data-penna-text>${text}</span>`;
    };

    const renderBadge = (tone, icon, text) => {
      if (tone) badge.dataset.tone = tone;
      else badge.removeAttribute("data-tone");
      badge.innerHTML = icon
        ? `<i data-icon="${icon}"></i><span class="badge-text" data-penna-text>${text}</span>`
        : `<span class="badge-text" data-penna-text>${text}</span>`;
    };

    const refresh = () => {
      window.pennaKit?.ceilText?.(card);
      window.pennaKit?.roundAll?.(card);
    };

    button.addEventListener("click", () => {
      if (button.hasAttribute("data-loading")) return;
      clearTimeout(timer);
      renderButton(true, "Saving");
      renderBadge("", "", "Loading");
      refresh();
      timer = setTimeout(() => {
        renderButton(false, "Save");
        renderBadge("success", "circle-check", "Saved");
        refresh();
        timer = setTimeout(() => {
          renderBadge("", "", "Rest");
          refresh();
        }, 1600);
      }, 900);
    });
  });
})();
