(() => {
  let active = false;
  let pinned = false;
  let target = null;
  let box = null;
  let boxTag = null;
  let hud = null;
  let statusBadge = null;
  let envEl = null;
  let gridEl = null;
  let hintEl = null;
  let copyBtn = null;
  let copyTimer = 0;
  let lastData = null;

  function ensureUi() {
    if (box && hud) return;
    box = document.createElement("div");
    box.className = "docs-insp-box";
    box.hidden = true;
    boxTag = document.createElement("span");
    boxTag.className = "docs-insp-tag";
    box.append(boxTag);
    document.body.append(box);

    hud = document.createElement("aside");
    hud.className = "docs-insp-hud";
    hud.setAttribute("data-penna", "popover");
    hud.setAttribute("data-ground", "");
    hud.setAttribute("role", "region");
    hud.setAttribute("aria-label", "Token inspector");
    hud.hidden = true;

    const head = document.createElement("div");
    head.className = "docs-insp-head";

    const titleWrap = document.createElement("div");
    titleWrap.className = "docs-insp-title-wrap";
    const icon = document.createElement("i");
    icon.dataset.icon = "layers";
    const title = document.createElement("span");
    title.className = "docs-insp-title";
    title.textContent = "Token inspector";
    statusBadge = document.createElement("span");
    statusBadge.className = "penna-badge";
    statusBadge.setAttribute("data-penna", "badge");
    statusBadge.setAttribute("data-size", "small");
    statusBadge.textContent = "Hover";
    titleWrap.append(icon, title, statusBadge);

    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "penna-button";
    closeBtn.setAttribute("data-penna", "button.quiet");
    closeBtn.setAttribute("data-shape", "icon");
    closeBtn.setAttribute("data-size", "small");
    closeBtn.setAttribute("aria-label", "Close token inspector");
    const closeIcon = document.createElement("i");
    closeIcon.dataset.icon = "xmark";
    closeBtn.append(closeIcon);
    closeBtn.addEventListener("click", close);

    head.append(titleWrap, closeBtn);

    envEl = document.createElement("div");
    envEl.className = "docs-insp-env";

    gridEl = document.createElement("dl");
    gridEl.className = "docs-insp-grid";

    const foot = document.createElement("div");
    foot.className = "docs-insp-foot";

    hintEl = document.createElement("span");
    hintEl.className = "docs-insp-hint";
    hintEl.textContent = "Click a surface to pin · Esc to close";

    copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.className = "penna-button";
    copyBtn.setAttribute("data-penna", "button");
    copyBtn.setAttribute("data-size", "small");
    copyBtn.textContent = "Copy JSON";
    copyBtn.addEventListener("click", copySnapshot);

    foot.append(hintEl, copyBtn);
    hud.append(head, envEl, gridEl, foot);
    document.body.append(hud);
  }

  function envSnapshot() {
    const root = document.documentElement;
    return {
      mode: window.pennaKit?.choice?.mode || "dark",
      density: root.dataset.density || "regular",
      glass: root.dataset.glass === "none" ? "none" : "liquid",
      motion: root.dataset.motion === "off" ? "off" : "default",
    };
  }

  function pickSurface(node) {
    if (!(node instanceof Element)) return null;
    if (hud?.contains(node) || box?.contains(node)) return null;
    return node.closest("[data-penna]") || node;
  }

  function nearestGround(el) {
    let cur = el.parentElement;
    while (cur) {
      if (cur.hasAttribute("data-ground")) {
        return cur.getAttribute("data-penna") || cur.className.split(/\s+/)[0] || "ground";
      }
      cur = cur.parentElement;
    }
    return "window";
  }

  function readToken(cs, name, fallback) {
    const val = cs.getPropertyValue(name).trim();
    return val || fallback;
  }

  function renderEnv(env) {
    if (!envEl) return;
    envEl.replaceChildren();
    const pairs = [
      ["Mode", env.mode],
      ["Density", env.density],
      ["Glass", env.glass],
      ["Motion", env.motion],
    ];
    for (const [k, v] of pairs) {
      const chip = document.createElement("span");
      chip.className = "docs-insp-pill";
      const label = document.createElement("span");
      label.className = "docs-insp-pill-k";
      label.textContent = k;
      const val = document.createElement("span");
      val.className = "docs-insp-pill-v";
      val.textContent = v;
      chip.append(label, val);
      envEl.append(chip);
    }
  }

  function addRow(key, value, swatchColor) {
    const dt = document.createElement("dt");
    dt.textContent = key;
    const dd = document.createElement("dd");
    if (swatchColor) {
      const sw = document.createElement("span");
      sw.className = "docs-insp-swatch";
      sw.style.background = swatchColor;
      dd.append(sw);
    }
    const text = document.createElement("span");
    text.className = "docs-insp-val";
    text.textContent = value;
    dd.append(text);
    gridEl.append(dt, dd);
  }

  function positionBox(el) {
    if (!box || !el || !el.isConnected) {
      if (box) box.hidden = true;
      return;
    }
    const rect = el.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) {
      box.hidden = true;
      return;
    }
    box.hidden = false;
    box.style.left = `${rect.left}px`;
    box.style.top = `${rect.top}px`;
    box.style.width = `${rect.width}px`;
    box.style.height = `${rect.height}px`;
    const role = el.getAttribute("data-penna") || el.tagName.toLowerCase();
    boxTag.textContent = `${role} · ${Math.round(rect.width)}×${Math.round(rect.height)}`;
    boxTag.dataset.flip = rect.top < 26 ? "inside" : "above";
  }

  function inspect(el) {
    const env = envSnapshot();
    renderEnv(env);
    target = el;
    if (!gridEl) return;
    gridEl.replaceChildren();

    if (!el || !el.isConnected) {
      positionBox(null);
      lastData = { env, target: null };
      addRow("Surface", "Hover any element");
      return;
    }

    positionBox(el);
    const cs = getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    const role = el.getAttribute("data-penna") || el.tagName.toLowerCase();
    const ground = nearestGround(el);
    const fill = readToken(cs, "--penna-fill", cs.backgroundColor);
    const ink = readToken(cs, "--penna-ink", cs.color);
    const radius = readToken(cs, "--penna-r", cs.borderRadius || "0px");
    const smoothing = readToken(cs, "--penna-k", "—");
    const glass = readToken(cs, "--penna-glass", "none");
    const tone =
      el.getAttribute("data-tone") ||
      (el.hasAttribute("data-prominent") ? "prominent" : "neutral");
    const size = `${Math.round(rect.width)} × ${Math.round(rect.height)} px`;

    addRow("Role", role);
    addRow("Ground", ground);
    addRow("Size", size);
    addRow("--penna-fill", fill, cs.backgroundColor);
    addRow("--penna-ink", ink, cs.color);
    addRow("--penna-r", `${radius} · k=${smoothing}`);
    addRow("Glass / Tone", `${glass} · ${tone}`);

    lastData = {
      env,
      target: {
        role,
        ground,
        width: Math.round(rect.width),
        height: Math.round(rect.height),
        fill,
        ink,
        radius,
        smoothing,
        glass,
        tone,
      },
    };
  }

  function updatePinState() {
    if (statusBadge) {
      statusBadge.textContent = pinned ? "Pinned" : "Hover";
      if (pinned) statusBadge.setAttribute("data-tone", "accent");
      else statusBadge.removeAttribute("data-tone");
    }
    if (hintEl) {
      hintEl.textContent = pinned
        ? "Pinned · Click any surface to repin"
        : "Click a surface to pin · Esc to close";
    }
    if (box) box.toggleAttribute("data-pinned", pinned);
  }

  async function copySnapshot() {
    if (!lastData) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(lastData, null, 2));
      if (copyBtn) {
        copyBtn.textContent = "Copied";
        copyBtn.setAttribute("data-copied", "");
        clearTimeout(copyTimer);
        copyTimer = setTimeout(() => {
          copyBtn.textContent = "Copy JSON";
          copyBtn.removeAttribute("data-copied");
        }, 1400);
      }
    } catch {}
  }

  function onPointerMove(event) {
    if (!active || pinned) return;
    const hit = pickSurface(event.target);
    if (hit && hit !== target) inspect(hit);
  }

  function onClick(event) {
    if (!active) return;
    if (hud?.contains(event.target)) return;
    const hit = pickSurface(event.target);
    if (!hit) return;
    event.preventDefault();
    event.stopPropagation();
    if (pinned && hit === target) {
      pinned = false;
    } else {
      pinned = true;
      inspect(hit);
    }
    updatePinState();
  }

  function onRelayout() {
    if (active && target) positionBox(target);
  }

  function onThemeChange() {
    if (active) inspect(target);
  }

  function open() {
    if (active) return;
    ensureUi();
    active = true;
    pinned = false;
    hud.hidden = false;
    document.documentElement.dataset.docsInspector = "on";
    updatePinState();
    const initial =
      document.querySelector(".penna-docs-page [data-penna]") ||
      document.querySelector("[data-penna]");
    inspect(initial);
    requestAnimationFrame(() => window.pennaKit?.settle?.(hud));
    document.addEventListener("pointermove", onPointerMove, { passive: true });
    document.addEventListener("click", onClick, true);
    window.addEventListener("scroll", onRelayout, true);
    window.addEventListener("resize", onRelayout);
    document.addEventListener("penna-theme", onThemeChange);
  }

  function close() {
    if (!active) return;
    active = false;
    pinned = false;
    target = null;
    if (hud) hud.hidden = true;
    if (box) box.hidden = true;
    delete document.documentElement.dataset.docsInspector;
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("click", onClick, true);
    window.removeEventListener("scroll", onRelayout, true);
    window.removeEventListener("resize", onRelayout);
    document.removeEventListener("penna-theme", onThemeChange);
  }

  function toggle() {
    if (active) close();
    else open();
  }

  document.addEventListener(
    "keydown",
    (event) => {
      if (
        event.key === "F12" ||
        (event.altKey && event.shiftKey && event.key.toLowerCase() === "i")
      ) {
        event.preventDefault();
        event.stopPropagation();
        toggle();
        return;
      }
      if (event.key === "Escape" && active) {
        event.preventDefault();
        event.stopPropagation();
        close();
      }
    },
    true,
  );

  window.pennaInspector = {
    open,
    close,
    toggle,
    isOpen: () => active,
  };
})();
