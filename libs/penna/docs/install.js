(() => {
  const root = document.querySelector("[data-docs-install]");
  if (!root) return;

  const targetSwitch = root.querySelector("[data-install-target]");
  const meta = root.querySelector("[data-install-meta]");
  const panels = [...root.querySelectorAll("[data-install-panel]")];
  const cargoCode = root.querySelector("[data-install-cargo]");
  const htmlCode = root.querySelector("[data-install-html]");

  const WIDGETS = {
    button: { css: "src/components/button/button.css", js: "src/components/button/button.js" },
    input: { css: "src/components/input/input.css", js: "src/components/input/input.js" },
    popover: { css: "src/components/popover/popover.css", js: null },
    switch: { css: "src/components/switch/switch.css", js: "src/components/switch/switch.js" },
    text: { css: "src/components/text/text.css", js: "src/components/text/text.js" },
  };

  function span(cls, text) {
    return `<span class="tk-${cls}">${text}</span>`;
  }

  function renderCargo() {
    if (!cargoCode) return;
    const picked = [...root.querySelectorAll("[data-install-feature]")]
      .filter((el) => el.getAttribute("aria-checked") === "true")
      .map((el) => el.dataset.installFeature);
    const lines = [
      `[${span("type", "dependencies")}]`,
      `${span("property", "ala")} = { ${span("property", "git")} = ${span("string", '"https://github.com/autistic-computer/ala"')} }`,
      `${span("property", "ala-platform")} = { ${span("property", "git")} = ${span("string", '"https://github.com/autistic-computer/ala"')} }`,
    ];
    if (picked.length) {
      lines.push(
        "",
        `[${span("type", "dependencies")}.${span("property", "penna")}]`,
        `${span("property", "git")} = ${span("string", '"https://github.com/autistic-computer/penna"')}`,
        `${span("property", "features")} = [${picked.map((name) => span("string", `"${name}"`)).join(", ")}]`,
      );
    } else {
      lines.push(
        `${span("property", "penna")} = { ${span("property", "git")} = ${span("string", '"https://github.com/autistic-computer/penna"')} }`,
      );
    }
    cargoCode.innerHTML = lines.join("\n");
  }

  function linkLine(href) {
    return `&lt;${span("tag", "link")} ${span("attribute", "rel")}=${span("string", '"stylesheet"')} ${span("attribute", "href")}=${span("string", `"${href}"`)} /&gt;`;
  }

  function scriptLine(src, extra = "") {
    return `&lt;${span("tag", "script")} ${span("attribute", "src")}=${span("string", `"${src}"`)}${extra}&gt;&lt;/${span("tag", "script")}&gt;`;
  }

  function renderHtml() {
    if (!htmlCode) return;
    const picked = [...root.querySelectorAll("[data-install-widget]")]
      .filter((el) => el.getAttribute("aria-checked") === "true")
      .map((el) => el.dataset.installWidget);
    const hideSwitcher = root.querySelector("[data-install-switcher]")?.getAttribute("aria-checked") === "true";
    const lines = [linkLine("web/penna.css")];
    for (const name of picked) {
      if (WIDGETS[name]?.css) lines.push(linkLine(WIDGETS[name].css));
    }
    if (hideSwitcher) {
      lines.push(
        `&lt;${span("tag", "script")} ${span("attribute", "src")}=${span("string", '"web/kit.js"')}\n  ${span("attribute", "data-tokens")}=${span("string", '"web/tokens/"')} ${span("attribute", "data-switcher")}=${span("string", '"off"')}&gt;&lt;/${span("tag", "script")}&gt;`,
      );
    } else {
      lines.push(
        scriptLine("web/kit.js", ` ${span("attribute", "data-tokens")}=${span("string", '"web/tokens/"')}`),
      );
    }
    for (const name of picked) {
      if (WIDGETS[name]?.js) lines.push(scriptLine(WIDGETS[name].js, ` ${span("attribute", "defer")}`));
    }
    htmlCode.innerHTML = lines.join("\n");
  }

  function showTarget(index) {
    const mode = index === 1 ? "html" : "rust";
    panels.forEach((panel) => {
      const open = panel.dataset.installPanel === mode;
      panel.hidden = !open;
      if (open) {
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(panel));
      }
    });
    if (meta) {
      meta.textContent = mode === "html" ? "No build step" : "Rust 1.99 · Edition 2024";
    }
  }

  targetSwitch?.addEventListener("change", (event) => {
    showTarget(event.detail.index);
  });

  root.querySelectorAll("[data-install-feature]").forEach((toggle) => {
    toggle.addEventListener("change", renderCargo);
  });

  root.querySelectorAll("[data-install-widget], [data-install-switcher]").forEach((toggle) => {
    toggle.addEventListener("change", renderHtml);
  });

  renderCargo();
  renderHtml();

  const win = document.querySelector("[data-install-win]");
  if (!win) return;

  const navRows = [...win.querySelectorAll("[data-win-tab]")];
  const views = [...win.querySelectorAll("[data-win-view]")];
  const titleEl = win.querySelector("[data-win-title]");
  const popHost = win.querySelector("[data-win-popover-host]");
  const popBtn = win.querySelector("[data-win-popover-btn]");
  const popPanel = win.querySelector("[data-win-popover]");
  const iconPick = win.querySelector("[data-win-icon-name]");
  const modeReadout = win.querySelector("[data-win-mode-readout]");

  function syncModeReadout() {
    if (!modeReadout) return;
    const mode = document.documentElement.dataset.pennaMode === "light" ? "Light" : "Dark";
    const glass = document.documentElement.dataset.glass === "none" ? "Solid" : "Liquid glass";
    modeReadout.textContent = `${mode} · ${glass}`;
  }

  function setPopOpen(open) {
    if (!popHost) return;
    popHost.toggleAttribute("data-open", open);
    popBtn?.setAttribute("aria-expanded", String(open));
    if (open && popPanel) {
      requestAnimationFrame(() => window.pennaKit?.settle?.(popPanel));
    }
  }

  function showTab(tab) {
    setPopOpen(false);
    navRows.forEach((row) => {
      const current = row.dataset.winTab === tab;
      if (current) {
        row.setAttribute("aria-current", "page");
        row.dataset.state = "selected";
      } else {
        row.removeAttribute("aria-current");
        delete row.dataset.state;
      }
    });
    views.forEach((view) => {
      const open = view.dataset.winView === tab;
      view.hidden = !open;
      if (open) requestAnimationFrame(() => window.pennaKit?.roundAll?.(view));
    });
    const label = navRows.find((row) => row.dataset.winTab === tab)?.dataset.winLabel;
    if (titleEl && label) titleEl.textContent = label;
  }

  navRows.forEach((row) => {
    const pick = () => showTab(row.dataset.winTab);
    row.addEventListener("click", pick);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        pick();
      }
    });
  });

  popBtn?.addEventListener("click", (event) => {
    event.stopPropagation();
    setPopOpen(!popHost?.hasAttribute("data-open"));
  });

  win.querySelector("[data-win-toggle-mode]")?.addEventListener("click", () => {
    pennaKit.choice.mode = pennaKit.choice.mode === "dark" ? "light" : "dark";
    pennaKit.apply();
  });

  win.querySelectorAll("[data-win-icon]").forEach((btn) => {
    btn.addEventListener("click", () => {
      win.querySelectorAll("[data-win-icon]").forEach((other) => {
        if (other === btn) other.dataset.state = "current";
        else delete other.dataset.state;
      });
      if (iconPick) iconPick.textContent = `IconName::${btn.dataset.winIcon}`;
    });
  });

  document.addEventListener("click", (event) => {
    if (popHost?.hasAttribute("data-open") && !popHost.contains(event.target)) {
      setPopOpen(false);
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && popHost?.hasAttribute("data-open")) {
      setPopOpen(false);
    }
  });

  document.addEventListener("penna-theme", syncModeReadout);
  syncModeReadout();
})();
