(() => {
  const root = document.querySelector("[data-docs-spacing]");
  if (!root) return;

  const stage = root.querySelector("[data-spacing-stage]");
  const overlay = root.querySelector("[data-spacing-overlay]");
  const toggle = root.querySelector("[data-spacing-xray]");

  function rect(el, origin) {
    const b = el.getBoundingClientRect();
    return {
      left: b.left - origin.left,
      top: b.top - origin.top,
      right: b.right - origin.left,
      bottom: b.bottom - origin.top,
      width: b.width,
      height: b.height,
    };
  }

  function makeBand(cls, x, y, w, h, label) {
    if (w <= 0 || h <= 0) return null;
    const div = document.createElement("div");
    div.className = `docs-sp-band ${cls}`;
    div.style.left = `${x.toFixed(1)}px`;
    div.style.top = `${y.toFixed(1)}px`;
    div.style.width = `${w.toFixed(1)}px`;
    div.style.height = `${h.toFixed(1)}px`;
    if (label) {
      const tag = document.createElement("span");
      tag.className = "docs-sp-tag";
      tag.textContent = label;
      div.append(tag);
    }
    return div;
  }

  function drawOverlay() {
    if (!stage || !overlay) return;
    overlay.replaceChildren();
    if (!root.hasAttribute("data-xray")) return;

    const origin = stage.getBoundingClientRect();
    const bands = [];

    const bar = stage.querySelector("[data-sp-bar]");
    if (bar) {
      const r = rect(bar, origin);
      const pad = 6;
      bands.push(makeBand("is-inset is-top", r.left, r.top, r.width, pad, "6 inset-chrome"));
      bands.push(makeBand("is-inset", r.left, r.bottom - pad, r.width, pad));
      bands.push(makeBand("is-inset", r.left, r.top + pad, pad, r.height - pad * 2));
      bands.push(makeBand("is-inset", r.right - pad, r.top + pad, pad, r.height - pad * 2));
    }

    const body = stage.querySelector("[data-sp-body]");
    if (body) {
      const r = rect(body, origin);
      const pad = 16;
      bands.push(makeBand("is-inset", r.left, r.top, r.width, pad, "16 inset-pane"));
      bands.push(makeBand("is-inset", r.left, r.bottom - pad, r.width, pad));
      bands.push(makeBand("is-inset", r.left, r.top + pad, pad, r.height - pad * 2));
      bands.push(makeBand("is-inset", r.right - pad, r.top + pad, pad, r.height - pad * 2));

      const groups = [...body.children];
      for (let i = 0; i < groups.length - 1; i += 1) {
        const a = rect(groups[i], origin);
        const b = rect(groups[i + 1], origin);
        const gap = Math.round(b.top - a.bottom);
        if (gap > 0) {
          bands.push(makeBand("is-gap", r.left + pad, a.bottom, r.width - pad * 2, b.top - a.bottom, `${gap} gap-group`));
        }
      }
    }

    const field = stage.querySelector("[data-sp-field]");
    if (field) {
      const label = field.querySelector(".penna-field-label");
      const input = field.querySelector(".penna-input");
      if (label && input) {
        const a = rect(label, origin);
        const b = rect(input, origin);
        const gap = Math.round(b.top - a.bottom);
        if (gap > 0) {
          bands.push(makeBand("is-gap", b.left, a.bottom, b.width, b.top - a.bottom, `${gap} gap-tight`));
        }
      }
    }

    const listCard = stage.querySelector("[data-sp-list]");
    if (listCard) {
      const r = rect(listCard, origin);
      const rows = [...listCard.querySelectorAll(".penna-list-item")];
      if (rows.length >= 2) {
        const a = rect(rows[0], origin);
        const b = rect(rows[1], origin);
        const gap = Math.round(b.top - a.bottom);
        if (gap > 0) {
          bands.push(makeBand("is-gap", a.left, a.bottom, a.width, b.top - a.bottom, `${gap}`));
        }
      }
      const pad = 6;
      bands.push(makeBand("is-inset", r.left, r.top, r.width, pad));
      bands.push(makeBand("is-inset", r.left, r.bottom - pad, r.width, pad));
      bands.push(makeBand("is-inset", r.left, r.top + pad, pad, r.height - pad * 2));
      bands.push(makeBand("is-inset is-v", r.right - pad, r.top + pad, pad, r.height - pad * 2, "6"));
    }

    const actions = stage.querySelector("[data-sp-actions]");
    if (actions) {
      const btns = [...actions.querySelectorAll("button")];
      if (btns.length >= 2) {
        const a = rect(btns[0], origin);
        const b = rect(btns[1], origin);
        const gap = Math.round(b.left - a.right);
        if (gap > 0) {
          bands.push(makeBand("is-gap is-v", a.right, a.top, b.left - a.right, a.height, `${gap}`));
        }
      }
    }

    overlay.append(...bands.filter(Boolean));
  }

  function syncXray(on) {
    root.toggleAttribute("data-xray", on);
    requestAnimationFrame(drawOverlay);
  }

  toggle?.addEventListener("change", (event) => syncXray(Boolean(event.detail?.checked)));

  const listItems = [...root.querySelectorAll("[data-sp-row]")];
  listItems.forEach((row) => {
    const pick = () => {
      listItems.forEach((other) => {
        const on = other === row;
        other.setAttribute("aria-selected", String(on));
        if (on) other.dataset.state = "selected";
        else other.removeAttribute("data-state");
      });
    };
    row.addEventListener("click", pick);
    row.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      pick();
    });
  });

  document.querySelectorAll("[data-copy-token]").forEach((btn) => {
    if (btn.dataset.copyWired) return;
    btn.dataset.copyWired = "1";
    let timer = null;
    const original = btn.textContent;
    btn.addEventListener("click", () => {
      const value = btn.dataset.copyToken;
      if (!value) return;
      navigator.clipboard?.writeText(value).catch(() => {});
      clearTimeout(timer);
      btn.dataset.copied = "";
      btn.textContent = "Copied";
      timer = setTimeout(() => {
        delete btn.dataset.copied;
        btn.textContent = original;
      }, 1200);
    });
  });

  if (stage && typeof ResizeObserver !== "undefined") {
    new ResizeObserver(() => requestAnimationFrame(drawOverlay)).observe(stage);
  }

  document.addEventListener("penna-theme", () => requestAnimationFrame(drawOverlay));
  document.fonts?.ready.then(() => requestAnimationFrame(drawOverlay));
  syncXray(true);
})();
