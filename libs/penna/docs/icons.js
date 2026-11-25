(() => {
  const root = document.querySelector("[data-docs-icons]");
  if (!root) return;

  const SIZES = [14, 16, 20, 24];

  const searchInput = root.querySelector("[data-ic-search]");
  const sizeSeg = root.querySelector("[data-ic-size]");
  const fmtSeg = root.querySelector("[data-ic-copy-fmt]");
  const groupsHost = root.querySelector("[data-ic-groups]");
  const gridHost = root.querySelector("[data-ic-grid]");
  const previewIcon = root.querySelector("[data-ic-preview]");
  const codeEl = root.querySelector("[data-ic-code]");
  const noteEl = root.querySelector("[data-ic-note]");
  const countEl = root.querySelector("[data-ic-count]");

  let allIcons = [];
  let activeGroup = "All";
  let copyFormat = "rust";
  let iconSize = 16;
  let copiedName = "";
  let copyTimer = 0;

  function snippet(icon) {
    return copyFormat === "html"
      ? `<i data-icon="${icon.name}"></i>`
      : `IconName::${icon.variant}`;
  }

  function setDetail(icon, copied) {
    if (!icon) return;
    if (previewIcon) previewIcon.dataset.icon = icon.name;
    if (codeEl) codeEl.textContent = copied ? `Copied ${snippet(icon)}` : snippet(icon);
    if (noteEl) noteEl.textContent = icon.note || `${icon.group} · <i data-icon="${icon.name}"></i>`;
  }

  function renderGroups() {
    if (!groupsHost) return;
    const counts = new Map();
    for (const icon of allIcons) {
      counts.set(icon.group, (counts.get(icon.group) || 0) + 1);
    }
    const groups = ["All", ...counts.keys()];
    const frag = document.createDocumentFragment();
    for (const group of groups) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "penna-chip";
      btn.setAttribute("data-penna", "chip");
      btn.setAttribute("data-interactive", "");
      if (group === activeGroup) btn.dataset.state = "selected";
      const label = document.createElement("span");
      label.className = "penna-chip-label";
      label.textContent = group === "All" ? `All (${allIcons.length})` : `${group} (${counts.get(group)})`;
      btn.append(label);
      btn.addEventListener("click", () => {
        activeGroup = group;
        renderGroups();
        renderGrid();
      });
      frag.append(btn);
    }
    groupsHost.replaceChildren(frag);
    requestAnimationFrame(() => window.pennaKit?.roundAll?.(groupsHost));
  }

  function renderGrid() {
    if (!gridHost) return;
    const q = (searchInput?.value || "").trim().toLowerCase();
    const list = allIcons.filter((icon) => {
      if (activeGroup !== "All" && icon.group !== activeGroup) return false;
      if (!q) return true;
      return (
        icon.name.toLowerCase().includes(q) ||
        icon.variant.toLowerCase().includes(q) ||
        icon.group.toLowerCase().includes(q) ||
        (icon.note && icon.note.toLowerCase().includes(q))
      );
    });
    if (countEl) countEl.textContent = `${list.length} of ${allIcons.length}`;
    gridHost.style.setProperty("--ic-size", `${iconSize}px`);

    const frag = document.createDocumentFragment();
    for (const icon of list) {
      const tile = document.createElement("button");
      tile.type = "button";
      tile.className = "docs-ic-tile";
      tile.setAttribute("data-penna", "card");
      tile.setAttribute("aria-label", `${icon.name} (${icon.variant})`);
      if (copiedName === icon.name) tile.setAttribute("data-copied", "");
      const glyph = document.createElement("i");
      glyph.dataset.icon = icon.name;
      const name = document.createElement("span");
      name.className = "docs-ic-name";
      name.textContent = copiedName === icon.name ? "Copied" : icon.name;
      tile.append(glyph, name);
      tile.addEventListener("mouseenter", () => {
        if (!copiedName) setDetail(icon, false);
      });
      tile.addEventListener("focus", () => {
        if (!copiedName) setDetail(icon, false);
      });
      tile.addEventListener("click", async () => {
        const text = snippet(icon);
        try {
          await navigator.clipboard.writeText(text);
        } catch {}
        copiedName = icon.name;
        setDetail(icon, true);
        renderGrid();
        clearTimeout(copyTimer);
        copyTimer = setTimeout(() => {
          copiedName = "";
          setDetail(icon, false);
          renderGrid();
        }, 1200);
      });
      frag.append(tile);
    }

    if (!list.length) {
      const empty = document.createElement("div");
      empty.className = "docs-ic-empty";
      empty.textContent = "No matching icons";
      frag.append(empty);
    }

    gridHost.replaceChildren(frag);
  }

  searchInput?.addEventListener("input", renderGrid);

  sizeSeg?.addEventListener("change", (event) => {
    iconSize = SIZES[event.detail.index] || 16;
    renderGrid();
  });

  fmtSeg?.addEventListener("change", (event) => {
    copyFormat = event.detail.index === 1 ? "html" : "rust";
    if (allIcons.length) setDetail(allIcons.find((i) => i.name === previewIcon?.dataset.icon) || allIcons[0], false);
  });

  fetch("assets/icons.json")
    .then((res) => (res.ok ? res.json() : []))
    .then((data) => {
      allIcons = Array.isArray(data) ? data : [];
      renderGroups();
      renderGrid();
      if (allIcons.length) setDetail(allIcons.find((i) => i.name === "bell") || allIcons[0], false);
      requestAnimationFrame(() => {
        window.pennaKit?.roundAll?.(root);
        window.pennaSegmented?.wire?.(root);
      });
    })
    .catch(() => {});
})();
