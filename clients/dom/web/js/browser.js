import { activeWindow, command, flush, state } from "./core.js";
import { beginPaneDrag, beginPip, contextMenu, floatRows, paneRenderer } from "./workspace.js";

const REASON = "Embedded Chromium needs the desktop app. Open this page in a browser tab.";

function escape(text) {
  return String(text).replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[char]);
}

function normalizeUrl(raw) {
  const value = (raw || "").trim();
  if (!value || value === "about:blank") return "about:blank";
  if (value.startsWith("http://") || value.startsWith("https://")) return value;
  if (value.startsWith("localhost") || value.startsWith("127.0.0.1")) return `http://${value}`;
  return `https://${value}`;
}

function hostLabel(url) {
  if (!url || url === "about:blank") return "New tab";
  try {
    const parsed = new URL(url);
    return parsed.host || url;
  } catch {
    return url.replace(/^https?:\/\//, "").split("/")[0] || url;
  }
}

function tabTitle(url) {
  if (!url || url === "about:blank") return "New tab";
  if (url.includes("autistic.computer")) return "autistic.computer";
  return hostLabel(url);
}

function createTab(id, url) {
  return { id, url, history: [url], cursor: 0 };
}

function browser(pane) {
  const initialUrls = pane.info?.browser?.tabs?.length ? pane.info.browser.tabs : ["http://localhost:3000"];
  const model = {
    tabs: initialUrls.map((url, index) => createTab(index + 1, url)),
    activeTab: Math.min(pane.info?.browser?.active || 0, Math.max(0, initialUrls.length - 1)),
    nextTabId: initialUrls.length + 1,
    picking: false,
    pickedSelector: null,
    zoomPercent: 100,
    clicks: 1,
    reloads: 0,
    remoteEgress: pane.id === 11,
  };

  pane.body.innerHTML = `
    <div class="browser-header" data-penna="bar.pane">
      <div class="browser-tab-row">
        <div class="browser-tabs-wrap">
          <div data-penna="button.title" class="pane-title browser-pane-title">
            <span class="title-icon"><i data-icon="globe" class="kind"></i><i data-icon="arrows-move" class="move"></i></span>
            <span class="title" data-text>Browser</span>
          </div>
          <div class="browser-tab-group">
            <div class="browser-tabs" data-penna="tab.strip" role="tablist"></div>
            <button data-penna="icon-button" class="browser-new-tab" title="New tab"><i data-icon="plus"></i></button>
          </div>
        </div>
        <div class="browser-pane-actions"></div>
      </div>
      <div class="browser-toolbar">
        <div class="browser-cluster">
          <button data-penna="icon-button" data-action="back" title="Back"><i data-icon="arrow-left"></i></button>
          <button data-penna="icon-button" data-action="forward" title="Forward"><i data-icon="arrow-right"></i></button>
          <button data-penna="icon-button" data-action="reload" title="Reload"><i data-icon="redo-2"></i></button>
        </div>
        <div class="browser-address" data-penna="field.address">
          <button data-penna="icon-button.flat" class="browser-site-btn" data-action="site" title="Site controls"><i data-icon="site-controls"></i></button>
          <input class="browser-url-input" placeholder="Search or enter address" spellcheck="false" autocomplete="off">
        </div>
        <div class="browser-cluster">
          <button data-penna="icon-button" data-action="picker" title="Pick an element"><i data-icon="inspector"></i></button>
          <button data-penna="icon-button" data-action="more" title="More browser actions"><i data-icon="ellipsis-vertical"></i></button>
        </div>
      </div>
    </div>
    <div class="browser-pick-bar" hidden>
      <i data-icon="inspector"></i>
      <span>Click an element to copy its selector</span>
      <button data-penna="button.quiet" data-size="small" data-action="cancel-pick">Cancel</button>
    </div>
    <div class="browser-picked-banner" hidden></div>
    <div class="browser-viewport"></div>`;

  const paneTitleEl = pane.body.querySelector(".browser-pane-title");
  const tabsEl = pane.body.querySelector(".browser-tabs");
  const newTabBtn = pane.body.querySelector(".browser-new-tab");
  const actionsEl = pane.body.querySelector(".browser-pane-actions");
  const backBtn = pane.body.querySelector('[data-action="back"]');
  const forwardBtn = pane.body.querySelector('[data-action="forward"]');
  const reloadBtn = pane.body.querySelector('[data-action="reload"]');
  const siteBtn = pane.body.querySelector('[data-action="site"]');
  const pickerBtn = pane.body.querySelector('[data-action="picker"]');
  const moreBtn = pane.body.querySelector('[data-action="more"]');
  const urlInput = pane.body.querySelector(".browser-url-input");
  const pickBar = pane.body.querySelector(".browser-pick-bar");
  const pickedBanner = pane.body.querySelector(".browser-picked-banner");
  const viewport = pane.body.querySelector(".browser-viewport");

  paneTitleEl.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    if (pane.floating) beginPip(pane, event);
    else beginPaneDrag(pane.id, event, false);
  });

  const currentTab = () => model.tabs[model.activeTab] || model.tabs[0];
  const currentUrl = () => currentTab()?.url || "about:blank";
  const writable = () => state.connected && !state.tree?.readOnly;

  function navigateTo(raw) {
    const url = normalizeUrl(raw);
    const tab = currentTab();
    if (tab && tab.url !== url) {
      tab.history = tab.history.slice(0, tab.cursor + 1);
      tab.history.push(url);
      tab.cursor = tab.history.length - 1;
      tab.url = url;
    }
    model.pickedSelector = null;
    if (writable()) command("set-browser-tabs", ["-t", `%${pane.id}`, url]);
    render();
  }

  function pickElement(selector) {
    if (!model.picking) return;
    model.picking = false;
    model.pickedSelector = selector;
    navigator.clipboard?.writeText(selector).catch(() => {});
    render();
  }

  function renderPaneActions() {
    const win = activeWindow();
    const lone = (win?.panes.length || 1) === 1;
    const zoomed = win?.zoomedPane === pane.id;
    paneTitleEl.toggleAttribute("data-movable", pane.floating ? writable() : writable() && !zoomed && !lone);
    const zoom = zoomed ? ["zoom", "shrink", "Unzoom"] : ["zoom", "expand", "Zoom"];
    const actions = pane.floating
      ? [["tile", "layout-columns", "Tile back"], zoom, ["close", "xmark", "Close pane"]]
      : [
          ["split-bottom", "panel-bottom", "Split bottom"],
          ["split-right", "panel-right", "Split right"],
          ...(lone || zoomed ? [] : [["float", "picture-in-picture", "Float"]]),
          ...(lone ? [] : [zoom]),
          ["close", "xmark", "Close pane"],
        ];
    actionsEl.replaceChildren(
      ...actions.map(([action, iconName, title]) => {
        const btn = document.createElement("button");
        btn.dataset.penna = "icon-button.bar";
        btn.title = title;
        btn.innerHTML = `<i data-icon="${iconName}"></i>`;
        btn.addEventListener("click", (event) => {
          event.stopPropagation();
          if (!writable()) return;
          state.client.pane_action(pane.id, action, action === "float" ? floatRows() : 0);
          flush();
        });
        return btn;
      }),
    );
  }

  function renderTabs() {
    tabsEl.replaceChildren(
      ...model.tabs.map((tab, index) => {
        const active = index === model.activeTab;
        const el = document.createElement("div");
        el.dataset.penna = "tab.page";
        el.className = "browser-tab";
        el.setAttribute("role", "tab");
        el.setAttribute("aria-selected", String(active));
        if (active) el.dataset.state = "current";
        el.innerHTML = `<i data-icon="globe" class="browser-tab-icon"></i><span class="browser-tab-label"></span>${model.tabs.length > 1 ? `<button data-penna="icon-button.row" data-inside class="browser-tab-close" title="Close tab"><i data-icon="xmark"></i></button>` : ""}`;
        el.querySelector(".browser-tab-label").textContent = tabTitle(tab.url);
        el.addEventListener("click", () => {
          model.activeTab = index;
          model.pickedSelector = null;
          render();
        });
        el.querySelector(".browser-tab-close")?.addEventListener("click", (event) => {
          event.stopPropagation();
          if (model.tabs.length <= 1) return;
          model.tabs.splice(index, 1);
          model.activeTab = Math.min(model.activeTab, model.tabs.length - 1);
          model.pickedSelector = null;
          render();
        });
        return el;
      }),
    );
  }

  function bindPickable(node, selector) {
    node.classList.toggle("is-pickable", model.picking);
    node.classList.toggle("is-picked", model.pickedSelector === selector);
    if (model.picking) {
      node.addEventListener("click", (event) => {
        event.stopPropagation();
        pickElement(selector);
      });
    }
  }

  function renderShowcasePage(url) {
    viewport.style.zoom = model.zoomPercent === 100 ? "" : `${model.zoomPercent}%`;
    if (!url || url === "about:blank") {
      const recent = ["http://localhost:3000", "https://autistic.computer"];
      viewport.innerHTML = `<div class="browser-start"><div class="browser-start-inner"><div class="browser-start-head penna-muted">Recent</div><div class="browser-recent-list"></div></div></div>`;
      const list = viewport.querySelector(".browser-recent-list");
      for (const item of recent) {
        const row = document.createElement("button");
        row.className = "browser-recent-row";
        row.dataset.penna = "menu-item";
        row.innerHTML = `<i data-icon="globe"></i><span>${escape(item)}</span>`;
        row.addEventListener("click", () => navigateTo(item));
        list.append(row);
      }
      return;
    }
    if (url.includes("autistic.computer")) {
      viewport.innerHTML = `
        <div class="browser-mock-page centered">
          <div class="browser-mock-card" data-penna="card.activity" data-selector="main.autistic-computer">
            <pre class="browser-ac-ascii">autistic ─┐
          ├─ .computer
    adhd ─┘</pre>
            <div class="browser-mock-sub penna-mono penna-muted">software on a spectrum</div>
            <div class="browser-ac-row">
              <span class="browser-ac-link penna-mono">-&gt; zebrilus /zz</span>
              <button data-penna="button" data-size="small" class="browser-ac-open"><i data-icon="external-link"></i><span>Open</span></button>
            </div>
          </div>
        </div>`;
      const card = viewport.querySelector(".browser-mock-card");
      bindPickable(card, "main.autistic-computer");
      viewport.querySelector(".browser-ac-open").addEventListener("click", (event) => {
        event.stopPropagation();
        if (model.picking) pickElement("button.open-ac");
        else window.open("https://autistic.computer", "_blank", "noopener");
      });
      return;
    }
    const badge = model.reloads > 0 ? `hmr #${model.reloads}` : model.remoteEgress ? "ssh -D" : "CEF";
    const heading = model.remoteEgress ? "Remote dev server, local GPU" : "Chromium on the same GPU surface";
    const subtitle = model.remoteEgress ? "Traffic tunnels through desktop over ssh -D." : "Split, float into PiP, or inspect elements.";
    viewport.innerHTML = `
      <div class="browser-mock-page">
        <div class="browser-mock-bar" data-penna="card.activity" data-selector="header.dev-bar">
          <div class="browser-mock-left">
            <span class="browser-live-dot"></span>
            <span class="browser-mock-host">${escape(hostLabel(url))}</span>
          </div>
          <div class="browser-mock-right">
            <span data-penna="chip" data-tone="accent" data-size="xsmall" class="browser-mock-chip">${escape(badge)}</span>
            <button data-penna="button" data-size="xsmall" class="browser-ping-btn">Ping (${model.clicks})</button>
          </div>
        </div>
        <div class="browser-mock-card" data-penna="card.activity" data-selector="main.app-preview">
          <div class="browser-mock-title">${escape(heading)}</div>
          <div class="browser-mock-sub penna-muted">${escape(subtitle)}</div>
        </div>
      </div>`;
    bindPickable(viewport.querySelector(".browser-mock-bar"), "header.dev-bar");
    bindPickable(viewport.querySelector(".browser-mock-card"), "main.app-preview");
    viewport.querySelector(".browser-ping-btn").addEventListener("click", (event) => {
      event.stopPropagation();
      if (model.picking) {
        pickElement("button.ping-demo");
        return;
      }
      model.clicks += 1;
      render();
    });
  }

  function renderDesktopNotice(url) {
    viewport.innerHTML = `
      <div class="browser-message">
        <i data-icon="globe"></i>
        <div class="browser-title" data-text>Browser</div>
        <div class="penna-muted reason">${escape(REASON)}</div>
        ${url && url !== "about:blank" ? `<button data-penna="button" class="browser-open"><i data-icon="external-link"></i><span data-text>Open in browser tab</span></button>` : ""}
      </div>`;
    viewport.querySelector(".browser-open")?.addEventListener("click", () => window.open(url, "_blank", "noopener"));
  }

  function render() {
    const tab = currentTab();
    const url = currentUrl();
    renderTabs();
    renderPaneActions();
    if (document.activeElement !== urlInput) urlInput.value = url === "about:blank" ? "" : url;
    backBtn.disabled = !(tab && tab.cursor > 0);
    forwardBtn.disabled = !(tab && tab.cursor + 1 < tab.history.length);
    reloadBtn.disabled = !state.showcase && !url;
    pickerBtn.disabled = !state.showcase;
    pickerBtn.dataset.state = model.picking ? "selected" : "";
    pickerBtn.title = model.picking ? "Cancel element picker" : "Pick an element";
    pickBar.hidden = !model.picking;
    if (model.pickedSelector) {
      pickedBanner.hidden = false;
      pickedBanner.innerHTML = `<div class="browser-notice" data-penna="inline-panel"><i data-icon="inspector"></i><span>Picked <code>${escape(model.pickedSelector)}</code> — selector copied</span><button data-penna="button.quiet" data-size="small" class="browser-dismiss-pick">Dismiss</button></div>`;
      pickedBanner.querySelector(".browser-dismiss-pick").addEventListener("click", () => {
        model.pickedSelector = null;
        render();
      });
    } else {
      pickedBanner.hidden = true;
      pickedBanner.replaceChildren();
    }
    if (state.showcase) renderShowcasePage(url);
    else renderDesktopNotice(url);
  }

  newTabBtn.addEventListener("click", () => {
    const id = model.nextTabId++;
    model.tabs.push(createTab(id, "about:blank"));
    model.activeTab = model.tabs.length - 1;
    model.pickedSelector = null;
    render();
    urlInput.focus();
  });
  backBtn.addEventListener("click", () => {
    const tab = currentTab();
    if (!tab || tab.cursor <= 0) return;
    tab.cursor -= 1;
    tab.url = tab.history[tab.cursor];
    model.pickedSelector = null;
    render();
  });
  forwardBtn.addEventListener("click", () => {
    const tab = currentTab();
    if (!tab || tab.cursor + 1 >= tab.history.length) return;
    tab.cursor += 1;
    tab.url = tab.history[tab.cursor];
    model.pickedSelector = null;
    render();
  });
  reloadBtn.addEventListener("click", () => {
    model.reloads += 1;
    render();
  });
  pickerBtn.addEventListener("click", () => {
    model.picking = !model.picking;
    if (model.picking) model.pickedSelector = null;
    render();
  });
  pane.body.querySelector('[data-action="cancel-pick"]').addEventListener("click", () => {
    model.picking = false;
    render();
  });
  siteBtn.addEventListener("click", (event) => {
    event.stopPropagation();
    const url = currentUrl();
    const secure = url.startsWith("https://");
    const rect = siteBtn.getBoundingClientRect();
    contextMenu(rect.left, rect.bottom + 4, [
      { label: hostLabel(url), disabled: true, icon: secure ? "lock" : "globe" },
      { label: secure ? "Connection is secure" : "Local or unencrypted connection", disabled: true },
    ]);
  });
  moreBtn.addEventListener("click", (event) => {
    event.stopPropagation();
    const url = currentUrl();
    const rect = moreBtn.getBoundingClientRect();
    contextMenu(rect.left, rect.bottom + 4, [
      { label: "Open in browser tab", icon: "external-link", disabled: !url || url === "about:blank", run: () => window.open(url, "_blank", "noopener") },
      { label: "Copy URL", icon: "copy", disabled: !url || url === "about:blank", run: () => navigator.clipboard?.writeText(url).catch(() => {}) },
      "---",
      { label: "Reload", icon: "redo-2", run: () => { model.reloads += 1; render(); } },
      { label: model.picking ? "Cancel element picker" : "Pick an element", icon: "inspector", disabled: !state.showcase, run: () => { model.picking = !model.picking; render(); } },
      "---",
      { label: `Zoom In (${model.zoomPercent}%)`, disabled: !state.showcase, run: () => { model.zoomPercent = Math.min(200, model.zoomPercent + 10); render(); } },
      { label: "Zoom Out", disabled: !state.showcase, run: () => { model.zoomPercent = Math.max(50, model.zoomPercent - 10); render(); } },
      { label: "Reset Zoom", disabled: !state.showcase, run: () => { model.zoomPercent = 100; render(); } },
    ]);
  });
  urlInput.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      navigateTo(urlInput.value);
      urlInput.blur();
    } else if (event.key === "Escape") {
      urlInput.value = currentUrl() === "about:blank" ? "" : currentUrl();
      urlInput.blur();
    }
  });

  render();

  return {
    update() {
      const serverTabs = pane.info?.browser?.tabs;
      if (!state.showcase && serverTabs?.length) {
        model.tabs = serverTabs.map((url, index) => createTab(index + 1, url));
        model.activeTab = Math.min(pane.info?.browser?.active || 0, model.tabs.length - 1);
      }
      render();
    },
    layout() {},
    destroy() {},
  };
}

paneRenderer("browser", browser);
