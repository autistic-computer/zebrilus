(() => {
  if (!document.querySelector("[data-docs-showcase]")) return;

  const MODES = ["dark", "light"];
  const DENSITIES = ["compact", "regular", "comfortable"];

  const modeSeg = document.querySelector("[data-sc-mode]");
  const densitySeg = document.querySelector("[data-sc-density]");
  const glassToggle = document.querySelector("[data-sc-glass]");

  function currentDensity() {
    const raw = document.documentElement.dataset.density;
    return raw === "compact" || raw === "comfortable" ? raw : "regular";
  }

  function syncToolbar() {
    const modeIdx = Math.max(0, MODES.indexOf(pennaKit.choice.mode));
    const densityIdx = Math.max(0, DENSITIES.indexOf(currentDensity()));
    const glassOn = document.documentElement.dataset.glass !== "none";
    if (modeSeg) {
      modeSeg.dataset.value = String(modeIdx);
      window.pennaSegmented?.set?.(modeSeg, modeIdx);
    }
    if (densitySeg) {
      densitySeg.dataset.value = String(densityIdx);
      window.pennaSegmented?.set?.(densitySeg, densityIdx);
    }
    if (glassToggle) {
      glassToggle.setAttribute("aria-checked", String(glassOn));
    }
  }

  modeSeg?.addEventListener("change", (event) => {
    const next = MODES[event.detail.index];
    if (next && next !== pennaKit.choice.mode) {
      pennaKit.choice.mode = next;
      pennaKit.apply();
    }
  });

  densitySeg?.addEventListener("change", (event) => {
    const next = DENSITIES[event.detail.index];
    if (next) pennaKit.setDensity(next);
  });

  glassToggle?.addEventListener("change", (event) => {
    pennaKit.setGlass(Boolean(event.detail.checked));
  });

  const WINDOWS = {
    hit: {
      chatName: "Fix stack hit tests",
      chatCwd: "~/dev/penna",
      termName: "cargo test",
      termCwd: "~/dev/penna",
      splitDefault: true,
      focusDefault: "chat",
      messages: [
        { role: "user", text: "The PiP hit box is reading the stack bounds instead of its own frame." },
        { role: "step", icon: "file", label: "Read", detail: "src/foundation/surface.rs" },
        { role: "step", icon: "pencil", label: "Edited", detail: "src/blocks/compact/mod.rs" },
        { role: "step", icon: "square-terminal", label: "cargo test", badge: "493 passed", tone: "success" },
        { role: "assistant", text: "Moved the occlusion box onto the floating card and pinned the hit-test bounds." },
      ],
      termLines: [
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> cargo test -p penna',
        '<span class="docs-zz-tg">   Compiling</span> penna v0.4.0',
        '<span class="docs-zz-tg">    Finished</span> test in 1.84s',
        '<span class="docs-zz-td">     Running src/lib.rs</span>',
        '<span class="docs-zz-tg">ok.</span> 493 passed; 0 failed',
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> git status -s',
        '<span class="docs-zz-ty"> M</span> src/blocks/compact/mod.rs',
        '<span class="docs-zz-ty"> M</span> src/foundation/surface.rs',
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> <span class="docs-zz-cur"> </span>',
      ],
    },
    glass: {
      chatName: "Audit glass contrast",
      chatCwd: "~/dev/penna",
      termName: "rg tint_floor",
      termCwd: "~/dev/penna",
      splitDefault: true,
      focusDefault: "chat",
      messages: [
        { role: "user", text: "Check every glass role clears 4.5:1 over both dark and light content." },
        { role: "step", icon: "file", label: "Read", detail: "src/foundation/interface_theme.rs" },
        { role: "step", icon: "square-terminal", label: "cargo test glass", badge: "needs approval", tone: "warning" },
        { role: "assistant", text: "Ready to run the contrast sweep across all floating surfaces once approved." },
      ],
      termLines: [
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> rg -n tint_floor src/',
        '<span class="docs-zz-tm">src/foundation/interface_theme.rs</span>:376',
        '<span class="docs-zz-tm">src/foundation/interface_theme.rs</span>:411',
        '<span class="docs-zz-td">TEXT_ON_GLASS = 4.5</span>',
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> <span class="docs-zz-cur"> </span>',
      ],
    },
    shell: {
      chatName: "Workspace notes",
      chatCwd: "~/dev/penna",
      termName: "fish",
      termCwd: "~/dev/penna",
      splitDefault: false,
      focusDefault: "term",
      messages: [
        { role: "assistant", text: "No active chat in this window. Split a pane or write below to start one." },
      ],
      termLines: [
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> cargo run --example docs',
        '<span class="docs-zz-td">writing 82 pages + llms.txt</span>',
        '<span class="docs-zz-tg">check ok:</span> 82 pages, 78 icons',
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> git log --oneline -4',
        '<span class="docs-zz-ty">565d411</span> widget micro-craft and inspector',
        '<span class="docs-zz-ty">8f9dfde</span> add 15 dev-tool components',
        '<span class="docs-zz-ty">e5c2c7e</span> add showcase and motion lab',
        '<span class="docs-zz-ty">36ccd6a</span> document blocks and llms.txt',
        '<span class="docs-zz-tc">zz</span> <span class="docs-zz-tg">❯</span> <span class="docs-zz-cur"> </span>',
      ],
    },
    release: {
      chatName: "Bump release notes",
      chatCwd: "~/dev/zebrilus",
      termName: "git diff",
      termCwd: "~/dev/zebrilus",
      splitDefault: true,
      focusDefault: "chat",
      messages: [
        { role: "user", text: "Summarize the penna token changes for the next release." },
        { role: "step", icon: "pencil", label: "Edited", detail: "CHANGELOG.md" },
        { role: "assistant", text: "Drafted the changelog entries covering the OKLCH data palette and token inspector." },
      ],
      termLines: [
        '<span class="docs-zz-tc">ubuntu</span> <span class="docs-zz-tg">❯</span> git diff --stat',
        ' CHANGELOG.md | 18 <span class="docs-zz-tg">++++++++++++++++++</span>',
        ' 1 file changed, 18 insertions(+)',
        '<span class="docs-zz-tc">ubuntu</span> <span class="docs-zz-tg">❯</span> <span class="docs-zz-cur"> </span>',
      ],
    },
  };

  const zzRoot = document.querySelector("[data-sc-zz]");
  if (zzRoot) {
    const winEl = zzRoot.querySelector("[data-sc-zz-win]");
    const viewSeg = zzRoot.querySelector("[data-sc-zz-view]");
    const splitButtons = [...zzRoot.querySelectorAll("[data-sc-zz-split]")];
    const sideToggles = [...zzRoot.querySelectorAll("[data-sc-zz-side-toggle]")];
    const stage = zzRoot.querySelector("[data-sc-zz-stage]");
    const viewPanes = [...zzRoot.querySelectorAll("[data-sc-zz-pane]")];
    const rows = [...zzRoot.querySelectorAll("[data-sc-win]")];
    const tabs = [...zzRoot.querySelectorAll("[data-sc-tab]")];
    const paneBoxes = [...zzRoot.querySelectorAll("[data-sc-zz-pane-box]")];
    const chatNameEl = zzRoot.querySelector("[data-sc-zz-chat-name]");
    const chatCwdEl = zzRoot.querySelector("[data-sc-zz-chat-cwd]");
    const termNameEl = zzRoot.querySelector("[data-sc-zz-term-name]");
    const termCwdEl = zzRoot.querySelector("[data-sc-zz-term-cwd]");
    const feedEl = zzRoot.querySelector("[data-sc-zz-feed]");
    const outEl = zzRoot.querySelector("[data-sc-zz-out]");
    const form = zzRoot.querySelector("[data-sc-zz-form]");
    const input = zzRoot.querySelector("[data-sc-zz-input]");
    const newChatBtn = zzRoot.querySelector("[data-sc-zz-new]");
    const hitGroup = zzRoot.querySelector('[data-sc-zz-group="hit"]');
    const hitNested = zzRoot.querySelector('[data-sc-zz-nested="hit"]');
    const hitChev = zzRoot.querySelector('[data-sc-zz-chev="hit"]');

    let currentKey = "hit";
    let focusedPane = "chat";
    let groupOpen = true;

    function buildBubble(text) {
      const wrap = document.createElement("div");
      wrap.className = "penna-chat-message";
      const card = document.createElement("div");
      card.className = "penna-card penna-chat-bubble";
      card.setAttribute("data-penna", "card.bubble");
      card.setAttribute("data-ground", "");
      const span = document.createElement("span");
      span.textContent = text;
      card.append(span);
      wrap.append(card);
      return wrap;
    }

    function buildStep(item) {
      const row = document.createElement("div");
      row.className = "penna-chat-activity";
      const glyph = document.createElement("span");
      glyph.className = "penna-chat-glyph";
      if (item.tone === "warning" || item.tone === "danger") glyph.dataset.tint = item.tone;
      const icon = document.createElement("i");
      icon.dataset.icon = item.icon;
      glyph.append(icon);
      const label = document.createElement("span");
      label.className = "penna-chat-label";
      label.textContent = item.label;
      row.append(glyph, label);
      if (item.detail) {
        const detail = document.createElement("span");
        detail.className = "penna-chat-detail";
        detail.textContent = item.detail;
        row.append(detail);
      }
      if (item.badge) {
        const badge = document.createElement("span");
        badge.className = "penna-badge";
        badge.setAttribute("data-penna", "badge");
        badge.setAttribute("data-size", "small");
        if (item.tone) badge.dataset.tone = item.tone;
        const bt = document.createElement("span");
        bt.className = "badge-text";
        bt.textContent = item.badge;
        badge.append(bt);
        row.append(badge);
      }
      return row;
    }

    function buildSay(text) {
      const p = document.createElement("p");
      p.className = "docs-zz-say";
      p.textContent = text;
      return p;
    }

    function renderFeed(messages) {
      if (!feedEl) return;
      const frag = document.createDocumentFragment();
      for (const item of messages) {
        if (item.role === "user") frag.append(buildBubble(item.text));
        else if (item.role === "step") frag.append(buildStep(item));
        else frag.append(buildSay(item.text));
      }
      feedEl.replaceChildren(frag);
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(feedEl));
    }

    function setActivePane(pane) {
      focusedPane = pane;
      paneBoxes.forEach((box) => {
        if (box.dataset.scZzPaneBox === pane) box.setAttribute("data-active", "");
        else box.removeAttribute("data-active");
      });
      syncRows();
    }

    function setSplit(split) {
      if (!stage) return;
      stage.dataset.split = String(split);
      splitButtons.forEach((btn) => btn.setAttribute("aria-pressed", String(split)));
      if (!split && focusedPane === "term" && currentKey !== "shell") {
        setActivePane("chat");
      }
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
    }

    function syncRows() {
      if (hitGroup) {
        hitGroup.toggleAttribute("data-lit", currentKey === "hit");
        hitGroup.toggleAttribute("data-open", groupOpen);
      }
      if (hitNested) hitNested.hidden = !groupOpen;
      if (hitChev) hitChev.setAttribute("aria-expanded", String(groupOpen));

      rows.forEach((row) => {
        const key = row.dataset.scWin;
        const focus = row.dataset.scFocus;
        let isCurrent = false;
        if (key === "hit") {
          if (focus) isCurrent = currentKey === "hit" && groupOpen && focusedPane === focus;
          else isCurrent = currentKey === "hit" && !groupOpen;
        } else {
          isCurrent = currentKey === key;
        }
        if (isCurrent) {
          row.dataset.state = "current";
          row.setAttribute("aria-current", "true");
        } else {
          delete row.dataset.state;
          row.removeAttribute("aria-current");
        }
      });

      tabs.forEach((tab) => {
        const active = tab.dataset.scTab === currentKey;
        tab.setAttribute("aria-selected", String(active));
        if (active) tab.dataset.state = "current";
        else delete tab.dataset.state;
      });
    }

    function selectWindow(key, focusOverride) {
      currentKey = WINDOWS[key] ? key : "hit";
      const data = WINDOWS[currentKey];
      const pane = focusOverride || data.focusDefault;
      if (chatNameEl) chatNameEl.textContent = data.chatName;
      if (chatCwdEl) chatCwdEl.textContent = data.chatCwd;
      if (termNameEl) termNameEl.textContent = data.termName;
      if (termCwdEl) termCwdEl.textContent = data.termCwd;
      renderFeed(data.messages);
      if (outEl) {
        outEl.innerHTML = data.termLines.map((line) => `<div>${line}</div>`).join("");
      }
      if (stage) {
        stage.dataset.single = currentKey === "shell" ? "term" : "chat";
        setSplit(data.splitDefault);
      }
      setActivePane(pane);
    }

    hitChev?.addEventListener("click", (event) => {
      event.stopPropagation();
      groupOpen = !groupOpen;
      syncRows();
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(winEl));
    });

    rows.forEach((row) => {
      const activate = () => selectWindow(row.dataset.scWin, row.dataset.scFocus);
      row.addEventListener("click", activate);
      row.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          activate();
        }
      });
    });

    tabs.forEach((tab) => {
      tab.addEventListener("click", () => selectWindow(tab.dataset.scTab));
    });

    paneBoxes.forEach((box) => {
      box.addEventListener("pointerdown", () => {
        if (box.dataset.scZzPaneBox) setActivePane(box.dataset.scZzPaneBox);
      });
    });

    sideToggles.forEach((btn) => {
      btn.addEventListener("click", () => {
        if (!winEl) return;
        const collapsed = winEl.dataset.sidebar === "collapsed";
        if (collapsed) delete winEl.dataset.sidebar;
        else winEl.dataset.sidebar = "collapsed";
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(winEl));
      });
    });

    splitButtons.forEach((btn) => {
      btn.addEventListener("click", () => {
        if (!stage) return;
        setSplit(stage.dataset.split !== "true");
      });
    });

    newChatBtn?.addEventListener("click", () => {
      selectWindow("hit", "chat");
      input?.focus();
    });

    viewSeg?.addEventListener("change", (event) => {
      const target = event.detail.index === 1 ? "phone" : "desktop";
      viewPanes.forEach((pane) => {
        pane.hidden = pane.dataset.scZzPane !== target;
      });
      splitButtons.forEach((btn) => {
        if (!btn.closest(".docs-zz-pacts")) btn.hidden = target === "phone";
      });
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(zzRoot));
    });

    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      const text = input?.value.trim();
      if (!text || !feedEl) return;
      const data = WINDOWS[currentKey];
      data.messages.push({ role: "user", text });
      data.messages.push({ role: "assistant", text: `Queued "${text}" in ${data.chatName}.` });
      renderFeed(data.messages);
      feedEl.scrollTop = feedEl.scrollHeight;
      input.value = "";
    });

    selectWindow("hit", "chat");
  }

  const HOSTS = {
    macbook: {
      name: "macbook",
      sub: "macOS · Primary · 1.2 ms",
      state: "Online",
      tone: "success",
      displays: "Built-in Liquid Retina 3456×2234",
      pointer: true,
      clipboard: true,
    },
    ubuntu: {
      name: "ubuntu",
      sub: "Linux · Left edge · 0.9 ms",
      state: "Online",
      tone: "success",
      displays: "DP-1 27″ 3840×2160 · HDMI-1 24″",
      pointer: true,
      clipboard: true,
    },
    thinkpad: {
      name: "thinkpad",
      sub: "Linux · Right edge · 2.4 ms",
      state: "Online",
      tone: "success",
      displays: "eDP-1 14″ 2880×1800",
      pointer: true,
      clipboard: false,
    },
    xps: {
      name: "xps",
      sub: "Linux · Nearby peer · mDNS",
      state: "Ready",
      tone: "warning",
      displays: "1 display · drag onto desk to pair",
      pointer: false,
      clipboard: true,
    },
    studio: {
      name: "studio",
      sub: "macOS · Nearby peer · mDNS",
      state: "Ready",
      tone: "warning",
      displays: "2 displays · Studio Display + Pro Display",
      pointer: false,
      clipboard: false,
    },
  };

  const nidusRoot = document.querySelector("[data-sc-nidus]");
  if (nidusRoot) {
    let activeHost = "macbook";
    const hostButtons = [...nidusRoot.querySelectorAll("[data-sc-host]")];
    const nameEl = nidusRoot.querySelector("[data-sc-nidus-name]");
    const subEl = nidusRoot.querySelector("[data-sc-nidus-sub]");
    const stateEl = nidusRoot.querySelector("[data-sc-nidus-state]");
    const dotEl = nidusRoot.querySelector("[data-sc-nidus-dot]");
    const displaysEl = nidusRoot.querySelector("[data-sc-nidus-displays]");
    const permPointer = nidusRoot.querySelector('[data-sc-perm="pointer"]');
    const permClipboard = nidusRoot.querySelector('[data-sc-perm="clipboard"]');

    function selectHost(key) {
      activeHost = key;
      const host = HOSTS[key] || HOSTS.macbook;
      hostButtons.forEach((btn) => {
        if (btn.dataset.scHost === key) btn.dataset.state = "selected";
        else delete btn.dataset.state;
      });
      if (nameEl) nameEl.textContent = host.name;
      if (subEl) subEl.textContent = host.sub;
      if (stateEl) stateEl.textContent = host.state;
      if (dotEl) dotEl.dataset.tone = host.tone;
      if (displaysEl) displaysEl.textContent = host.displays;
      if (permPointer) permPointer.setAttribute("aria-checked", String(host.pointer));
      if (permClipboard) permClipboard.setAttribute("aria-checked", String(host.clipboard));
    }

    hostButtons.forEach((btn) => {
      btn.addEventListener("click", () => selectHost(btn.dataset.scHost));
    });
    permPointer?.addEventListener("change", (event) => {
      HOSTS[activeHost].pointer = Boolean(event.detail.checked);
    });
    permClipboard?.addEventListener("change", (event) => {
      HOSTS[activeHost].clipboard = Boolean(event.detail.checked);
    });
    selectHost("macbook");
  }

  const parityRoot = document.querySelector("[data-sc-parity]");
  if (parityRoot) {
    const kpisHost = parityRoot.querySelector("[data-sc-parity-kpis]");
    const listHost = parityRoot.querySelector("[data-sc-parity-list]");
    const filterInput = parityRoot.querySelector("[data-sc-parity-filter]");
    const sortSeg = parityRoot.querySelector("[data-sc-parity-sort]");
    let report = null;
    let sortMode = 0;

    const STORY_LINKS = {
      appearance: "appearance.html",
      buttons: "button.html",
      canvas: "dock.html",
      chat: "chat.html",
      checks: "check.html",
      code_editor: "code-editor.html",
      color_picker: "color-picker.html",
      compact: "zoom-card.html",
      dialogs: "dialog.html",
      display: "badge.html",
      feedback: "empty-state.html",
      icons: "icon.html",
      inputs: "input.html",
      key_field: "key-field.html",
      markdown: "text.html",
      menus: "menu.html",
      navigation: "sidebar.html",
      notifications: "notification.html",
      palette: "palette.html",
      panels: "panel.html",
      phone: "phone.html",
      rows: "row.html",
      scrollbars: "scroll.html",
      selection: "select.html",
      settings: "settings.html",
      sheet: "sheet.html",
      stack_menu: "stack-menu.html",
      status: "notice.html",
      title_bar: "title-bar.html",
      which_key: "which-key.html",
    };

    function renderKpis(data) {
      if (!kpisHost) return;
      const above95 = data.stories.filter((s) => s.mean >= 95).length;
      const cards = [
        { label: "Overall parity", value: `${data.overall_mean.toFixed(2)}%` },
        { label: "Baseline checks", value: data.total_checks.toLocaleString() },
        { label: "Stories", value: String(data.story_count) },
        { label: "95%+ mean", value: `${above95} / ${data.story_count}` },
      ];
      kpisHost.replaceChildren(
        ...cards.map((item) => {
          const card = document.createElement("div");
          card.className = "docs-sc-kpi";
          card.setAttribute("data-penna", "card");
          card.setAttribute("data-ground", "");
          const val = document.createElement("span");
          val.className = "docs-sc-kpi-val";
          val.textContent = item.value;
          const lbl = document.createElement("span");
          lbl.className = "docs-sc-kpi-label";
          lbl.textContent = item.label;
          card.append(val, lbl);
          return card;
        }),
      );
    }

    function renderList() {
      if (!report || !listHost) return;
      const q = (filterInput?.value || "").trim().toLowerCase();
      let rows = report.stories.filter((s) => !q || s.story.toLowerCase().includes(q));
      if (sortMode === 1) rows = [...rows].sort((a, b) => b.mean - a.mean);
      else if (sortMode === 2) rows = [...rows].sort((a, b) => a.mean - b.mean);
      const frag = document.createDocumentFragment();
      for (const item of rows) {
        const href = STORY_LINKS[item.story];
        const row = document.createElement(href ? "a" : "div");
        row.className = "docs-sc-prow";
        if (href) row.href = href;
        const name = document.createElement("span");
        name.className = "docs-sc-pname";
        name.textContent = item.story.replaceAll("_", "-");
        const barWrap = document.createElement("span");
        barWrap.className = "docs-sc-pbar";
        const prog = document.createElement("div");
        prog.className = "penna-progress";
        prog.dataset.size = "small";
        const fill = document.createElement("div");
        fill.className = "penna-progress-fill";
        fill.style.setProperty("--penna-value", `${Math.min(100, Math.max(0, item.mean))}%`);
        if (item.mean >= 95) fill.style.background = "var(--penna-success)";
        else if (item.mean < 85) fill.style.background = "var(--penna-warning)";
        prog.append(fill);
        barWrap.append(prog);
        const checks = document.createElement("span");
        checks.className = "docs-sc-pmeta";
        checks.textContent = `${item.checks} checks`;
        const score = document.createElement("span");
        score.className = "docs-sc-pscore";
        score.textContent = `${item.mean.toFixed(1)}%`;
        row.append(name, barWrap, checks, score);
        frag.append(row);
      }
      if (!rows.length) {
        const empty = document.createElement("div");
        empty.className = "docs-sc-pempty";
        empty.textContent = "No matching stories";
        frag.append(empty);
      }
      listHost.replaceChildren(frag);
    }

    filterInput?.addEventListener("input", renderList);
    sortSeg?.addEventListener("change", (event) => {
      sortMode = event.detail.index;
      renderList();
    });

    fetch("assets/parity.json")
      .then((res) => (res.ok ? res.json() : null))
      .then((data) => {
        if (!data) return;
        report = data;
        renderKpis(data);
        renderList();
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(parityRoot));
      })
      .catch(() => {});
  }

  document.addEventListener("penna-theme", syncToolbar);
  syncToolbar();
  requestAnimationFrame(() => {
    window.pennaKit?.roundAll?.(document);
    window.pennaSegmented?.wire?.(document);
  });
})();
