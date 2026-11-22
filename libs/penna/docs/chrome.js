(() => {
  const MODES = ["dark", "light"];
  const DENSITIES = ["compact", "regular", "comfortable"];
  const GROUP_ICONS = {
    "Getting started": "sparkle",
    Foundations: "palette",
    Components: "layers",
    Blocks: "layout-dashboard",
  };

  const lookRoot = document.querySelector("[data-docs-look]");
  const lookTrigger = lookRoot?.querySelector("[data-docs-look-trigger]");
  const lookPanel = lookRoot?.querySelector(".docs-look-panel") ?? lookRoot?.panel;
  const modeControl = lookPanel?.querySelector('[data-docs-opt="mode"]');
  const glassControl = lookPanel?.querySelector('[data-docs-opt="glass"]');
  const densityControl = lookPanel?.querySelector('[data-docs-opt="density"]');

  const searchTrigger = document.querySelector("[data-docs-search]");
  const backdrop = document.querySelector("[data-docs-palette]");
  const surface = backdrop?.querySelector(".penna-palette-surface");
  const input = backdrop?.querySelector(".docs-palette-input");
  const rowsHost = backdrop?.querySelector(".penna-palette-rows");

  const searchKbd = searchTrigger?.querySelector("[data-kbd]");
  if (searchKbd && window.pennaKbd && !window.pennaKbd.mac) {
    searchKbd.dataset.kbd = "ctrl-k";
    window.pennaKbd.fill(searchTrigger);
  }

  function currentDensity() {
    const raw = document.documentElement.dataset.density;
    return raw === "compact" || raw === "comfortable" ? raw : "regular";
  }

  function currentGlass() {
    return document.documentElement.dataset.glass !== "none";
  }

  function syncAppearance() {
    const modeIdx = Math.max(0, MODES.indexOf(pennaKit.choice.mode));
    const densityIdx = Math.max(0, DENSITIES.indexOf(currentDensity()));
    if (modeControl) {
      modeControl.dataset.value = String(modeIdx);
      window.pennaSegmented?.set(modeControl, modeIdx);
    }
    if (glassControl) {
      glassControl.setAttribute("aria-checked", String(currentGlass()));
    }
    if (densityControl) {
      densityControl.dataset.value = String(densityIdx);
      window.pennaSegmented?.set(densityControl, densityIdx);
    }
  }

  function setLookOpen(open) {
    if (!lookRoot) return;
    if (open && lookPanel && !lookPanel.isConnected) lookRoot.append(lookPanel);
    lookRoot.toggleAttribute("data-open", open);
    lookTrigger?.setAttribute("aria-expanded", String(open));
    if (open && lookPanel) {
      syncAppearance();
      requestAnimationFrame(() => {
        window.pennaSegmented?.wire(lookPanel);
        window.pennaKit?.roundAll?.(lookPanel);
      });
    }
  }

  lookTrigger?.addEventListener("click", (event) => {
    event.stopPropagation();
    setLookOpen(!lookRoot?.hasAttribute("data-open"));
  });

  modeControl?.addEventListener("change", (event) => {
    const next = MODES[event.detail.index];
    if (!next || next === pennaKit.choice.mode) return;
    pennaKit.choice.mode = next;
    pennaKit.apply();
  });

  glassControl?.addEventListener("change", (event) => {
    pennaKit.setGlass(Boolean(event.detail.checked));
  });

  densityControl?.addEventListener("change", (event) => {
    const next = DENSITIES[event.detail.index];
    if (!next) return;
    pennaKit.setDensity(next);
  });

  document.addEventListener("penna-theme", () => {
    syncAppearance();
    if (lookRoot?.hasAttribute("data-open") && lookPanel) {
      requestAnimationFrame(() => {
        window.pennaSegmented?.wire(lookPanel);
        window.pennaKit?.roundAll?.(lookPanel);
      });
    }
  });

  document.addEventListener("click", (event) => {
    if (lookRoot?.hasAttribute("data-open") && !lookRoot.contains(event.target)) {
      setLookOpen(false);
    }
  });

  let indexPromise = null;
  let items = [];
  let activeIndex = 0;
  let lastFocused = null;

  function loadIndex() {
    indexPromise ??= fetch("assets/search.json")
      .then((response) => (response.ok ? response.json() : []))
      .catch(() => []);
    return indexPromise;
  }

  function commands() {
    const nextMode = pennaKit.choice.mode === "dark" ? "light" : "dark";
    const glassOn = currentGlass();
    return [
      {
        kind: "action",
        title: `Switch to ${nextMode} mode`,
        detail: "Appearance",
        icon: nextMode === "light" ? "sun" : "moon",
        keywords: "dark light mode color scheme",
        run: () => {
          pennaKit.choice.mode = nextMode;
          pennaKit.apply();
        },
      },
      {
        kind: "action",
        title: glassOn ? "Turn liquid glass off" : "Turn liquid glass on",
        detail: glassOn ? "Solid surfaces" : "Blur, tint and rim",
        icon: "sparkle",
        keywords: "glass liquid none blur transparency",
        run: () => pennaKit.setGlass(!glassOn),
      },
      {
        kind: "action",
        title: "Density: compact",
        detail: "24px rows and controls",
        icon: "sliders",
        keywords: "density compact size small",
        run: () => pennaKit.setDensity("compact"),
      },
      {
        kind: "action",
        title: "Density: regular",
        detail: "28px default rhythm",
        icon: "sliders",
        keywords: "density regular medium default",
        run: () => pennaKit.setDensity("regular"),
      },
      {
        kind: "action",
        title: "Density: comfortable",
        detail: "32px generous spacing",
        icon: "sliders",
        keywords: "density comfortable large room",
        run: () => pennaKit.setDensity("comfortable"),
      },
    ];
  }

  function matchSubstring(text, query) {
    if (!query) return null;
    const hay = text.toLowerCase();
    const needle = query.toLowerCase();
    const exact = hay.indexOf(needle);
    if (exact < 0) return null;
    const bonus = exact === 0 ? 120 : hay[exact - 1] === " " || hay[exact - 1] === "-" ? 90 : 60;
    return { score: bonus - exact * 0.25, ranges: [[exact, exact + needle.length]] };
  }

  function matchText(text, query) {
    if (!query) return { score: 1, ranges: [] };
    const sub = matchSubstring(text, query);
    if (sub) return sub;
    const hay = text.toLowerCase();
    const needle = query.toLowerCase();
    let qi = 0;
    let score = 0;
    let streak = 0;
    let gaps = 0;
    const ranges = [];
    for (let i = 0; i < hay.length && qi < needle.length; i++) {
      if (hay[i] === needle[qi]) {
        const boundary = i === 0 || hay[i - 1] === " " || hay[i - 1] === "-";
        streak += 1;
        score += 8 + streak * 4 + (boundary ? 12 : 0);
        const prev = ranges[ranges.length - 1];
        if (prev && prev[1] === i) prev[1] = i + 1;
        else ranges.push([i, i + 1]);
        qi += 1;
      } else {
        if (qi > 0) gaps += 1;
        streak = 0;
      }
    }
    if (qi < needle.length || gaps > needle.length * 3) return null;
    return { score: score - gaps * 1.5 - hay.length * 0.1, ranges };
  }

  function appendHighlighted(parent, text, ranges) {
    if (!ranges || !ranges.length) {
      parent.append(document.createTextNode(text));
      return;
    }
    let cursor = 0;
    for (const [start, end] of ranges) {
      if (start > cursor) parent.append(document.createTextNode(text.slice(cursor, start)));
      const hit = document.createElement("span");
      hit.className = "penna-palette-match";
      hit.textContent = text.slice(start, end);
      parent.append(hit);
      cursor = end;
    }
    if (cursor < text.length) parent.append(document.createTextNode(text.slice(cursor)));
  }

  function buildResults(pages, rawQuery) {
    const query = rawQuery.trim();
    const actions = commands();
    if (!query) {
      const currentSlug = document.querySelector(".penna-docs-page")?.dataset.page || "introduction";
      const picks = ["introduction", "foundations", "look", "button", "popover", "palette", "chat"]
        .filter((slug) => slug !== currentSlug)
        .map((slug) => pages.find((page) => page.slug === slug))
        .filter(Boolean)
        .slice(0, 6)
        .map((page) => ({
          kind: "page",
          title: page.title,
          detail: page.summary || page.group,
          right: page.group,
          icon: GROUP_ICONS[page.group] || "file",
          href: `${page.slug}.html`,
          ranges: [],
        }));
      return [
        { section: "Pages", rows: picks },
        { section: "Actions", rows: actions.slice(0, 4).map((cmd) => ({ ...cmd, ranges: [] })) },
      ];
    }

    const pageHits = [];
    const sectionHits = [];
    for (const page of pages) {
      const titleHit = matchText(page.title, query);
      const groupHit = matchSubstring(page.group, query);
      const summaryHit = page.summary ? matchSubstring(page.summary, query) : null;
      if (titleHit || groupHit || summaryHit) {
        const score = (titleHit ? titleHit.score + 100 : 0) + (groupHit ? groupHit.score * 0.35 : 0) + (summaryHit ? summaryHit.score * 0.45 : 0);
        pageHits.push({
          kind: "page",
          title: page.title,
          detail: page.summary || page.group,
          right: page.group,
          icon: GROUP_ICONS[page.group] || "file",
          href: `${page.slug}.html`,
          ranges: titleHit?.ranges || [],
          score,
        });
      }
      for (const heading of page.headings || []) {
        const headHit = matchText(heading.text, query);
        const comboHit = headHit ? null : matchSubstring(`${page.title} ${heading.text}`, query);
        if (headHit || comboHit) {
          sectionHits.push({
            kind: "section",
            prefix: `${page.title} / `,
            title: heading.text,
            detail: page.group,
            icon: "list",
            href: `${page.slug}.html#${heading.id}`,
            ranges: headHit?.ranges || [],
            score: headHit ? headHit.score + 85 : comboHit.score + 30,
          });
        }
      }
    }

    const actionHits = [];
    for (const cmd of actions) {
      const direct = matchText(cmd.title, query);
      const kw = direct ? null : matchSubstring(cmd.keywords, query);
      if (direct || kw) {
        actionHits.push({ ...cmd, ranges: direct?.ranges || [], score: direct ? direct.score + 75 : kw.score + 15 });
      }
    }

    pageHits.sort((a, b) => b.score - a.score);
    sectionHits.sort((a, b) => b.score - a.score);
    actionHits.sort((a, b) => b.score - a.score);

    const groups = [];
    if (pageHits.length) groups.push({ section: "Pages", rows: pageHits.slice(0, 8), top: pageHits[0].score });
    if (sectionHits.length) groups.push({ section: "Sections", rows: sectionHits.slice(0, 6), top: sectionHits[0].score });
    if (actionHits.length) groups.push({ section: "Actions", rows: actionHits.slice(0, 4), top: actionHits[0].score });
    groups.sort((a, b) => b.top - a.top);
    return groups;
  }

  function updateSelection(next) {
    if (!rowsHost) return;
    const rows = [...rowsHost.querySelectorAll(".penna-palette-row")];
    if (!rows.length) {
      activeIndex = 0;
      return;
    }
    activeIndex = ((next % rows.length) + rows.length) % rows.length;
    rows.forEach((row, index) => {
      const selected = index === activeIndex;
      row.setAttribute("aria-selected", String(selected));
      if (selected) row.setAttribute("data-state", "selected");
      else row.removeAttribute("data-state");
    });
    rows[activeIndex]?.scrollIntoView({ block: "nearest" });
  }

  function runItem(item) {
    if (!item) return;
    closePalette();
    if (item.href) location.href = item.href;
    else item.run?.();
  }

  function renderPalette(pages, query) {
    if (!rowsHost) return;
    const groups = buildResults(pages, query);
    items = [];
    const frag = document.createDocumentFragment();
    for (const group of groups) {
      const header = document.createElement("div");
      header.className = "penna-palette-section";
      const label = document.createElement("span");
      label.setAttribute("data-penna-text", "");
      label.textContent = group.section;
      header.append(label);
      frag.append(header);

      for (const entry of group.rows) {
        const index = items.length;
        items.push(entry);
        const row = document.createElement("div");
        row.className = "penna-list-item penna-palette-row";
        row.setAttribute("data-penna", "menu-item");
        row.setAttribute("role", "option");
        row.setAttribute("data-detail", "");
        row.setAttribute("aria-selected", String(index === 0));
        if (index === 0) row.setAttribute("data-state", "selected");

        const content = document.createElement("div");
        content.className = "penna-palette-content";

        const icon = document.createElement("i");
        icon.className = "penna-palette-icon";
        icon.dataset.icon = entry.icon;
        content.append(icon);

        const labelSpan = document.createElement("span");
        labelSpan.className = "penna-palette-label";
        if (entry.prefix) {
          const muted = document.createElement("span");
          muted.className = "penna-palette-muted";
          muted.textContent = entry.prefix;
          labelSpan.append(muted);
        }
        appendHighlighted(labelSpan, entry.title, entry.ranges);
        content.append(labelSpan);

        if (entry.detail) {
          const detail = document.createElement("span");
          detail.className = "penna-palette-detail";
          detail.textContent = entry.detail;
          content.append(detail);
        }

        if (entry.right) {
          const right = document.createElement("span");
          right.className = "penna-palette-right";
          right.textContent = entry.right;
          content.append(right);
        }

        row.append(content);
        row.addEventListener("pointermove", () => {
          if (activeIndex !== index) updateSelection(index);
        });
        row.addEventListener("click", () => runItem(entry));
        frag.append(row);
      }
    }

    if (!items.length) {
      const empty = document.createElement("div");
      empty.className = "penna-palette-empty";
      empty.textContent = "No matching pages or commands";
      frag.append(empty);
    }

    rowsHost.replaceChildren(frag);
    activeIndex = 0;
    if (surface) {
      requestAnimationFrame(() => {
        rowsHost.querySelectorAll("[data-penna]").forEach((el) => window.pennaKit?.settle?.(el));
        window.pennaKit?.settle?.(surface);
      });
    }
  }

  function openPalette() {
    if (!backdrop || !input) return;
    setLookOpen(false);
    lastFocused = document.activeElement;
    backdrop.hidden = false;
    input.value = "";
    input.focus();
    window.pennaKbd?.fill(backdrop);
    loadIndex().then((pages) => renderPalette(pages, ""));
    if (surface) {
      requestAnimationFrame(() => window.pennaKit?.settle?.(surface));
    }
  }

  function closePalette() {
    if (!backdrop || backdrop.hidden) return;
    backdrop.hidden = true;
    if (lastFocused instanceof HTMLElement && lastFocused.isConnected) lastFocused.focus();
    lastFocused = null;
  }

  searchTrigger?.addEventListener("click", openPalette);

  backdrop?.addEventListener("click", (event) => {
    if (event.target === backdrop) closePalette();
  });

  input?.addEventListener("input", () => {
    loadIndex().then((pages) => renderPalette(pages, input.value));
  });

  input?.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      event.stopPropagation();
      updateSelection(activeIndex + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      updateSelection(activeIndex - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      runItem(items[activeIndex]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closePalette();
    }
  });

  document.addEventListener("keydown", (event) => {
    const key = event.key.toLowerCase();
    if ((event.metaKey || event.ctrlKey) && key === "k") {
      event.preventDefault();
      if (backdrop && !backdrop.hidden) closePalette();
      else openPalette();
      return;
    }
    if (event.key === "Escape" && lookRoot?.hasAttribute("data-open")) {
      event.preventDefault();
      setLookOpen(false);
      lookTrigger?.focus();
      return;
    }
    if (
      event.key === "/" &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      backdrop?.hidden &&
      !(event.target instanceof HTMLElement && event.target.closest('input, textarea, select, [contenteditable="true"]'))
    ) {
      event.preventDefault();
      openPalette();
    }
  });

  syncAppearance();
})();
