import { $, emit, flush, markTree, on, onKey, phase, state, switchHost } from "./core.js";
import { appLogoSrc, contextMenu, narrow, openAppMenu, paintTile, paneIcon, preferences, programMaskHtml, setPreferences, sidebarInline } from "./workspace.js";

const app = $(".app");
const nav = $("nav[data-penna='sidebar']");
const list = $("#tree");
const logoImg = $("#chrome-logo");
const appMenuButton = $("#app-menu");
const toggleButton = $("#toggle-sidebar");
const viewButton = $("#flat-list");
const actions = $("#sidebar-actions");
const scrim = document.createElement("div");
scrim.className = "sidebar-scrim";
scrim.hidden = true;
app.append(scrim);
const handle = document.createElement("div");
handle.className = "sidebar-resize";
nav.append(handle);
const guides = document.createElement("div");
guides.className = "tree-guides";
nav.tabIndex = -1;

let slideover = false;
const flatFolds = {};

const NODE_ICONS = { host: "hard-drive", session: "layers", window: "app-window" };

function hostMark(platform) {
  return platform ? `<i data-icon="host-${platform}"></i>` : `<i class="host-logo"></i>`;
}

function writable() {
  return state.connected && !state.tree?.readOnly;
}

function focused() {
  return nav.contains(document.activeElement);
}

function width() {
  if (narrow()) return Math.min(256, innerWidth - 40);
  return Math.min(Math.max(preferences.sidebar_width, 160), Math.min(Math.max(innerWidth * 0.5, 160), 640));
}

function applyLayout() {
  const inline = sidebarInline();
  app.style.setProperty("--penna-sidebar-width", inline ? `${width()}px` : "0px");
  nav.style.width = `${width()}px`;
  nav.toggleAttribute("data-slideover", !inline && slideover);
  nav.hidden = !inline && !slideover;
  nav.toggleAttribute("data-divider", !(preferences.gaps && inline));
  scrim.hidden = inline || !slideover;
  viewButton.title = preferences.flat_sidebar ? "Show the session tree" : "Show chats";
  viewButton.querySelector("[data-icon]").dataset.icon = preferences.flat_sidebar ? "chat" : "layers";
  if (logoImg) logoImg.src = appLogoSrc();
}

function hoverFill(row) {
  const fill = row.querySelector(".flat-fill");
  row.addEventListener("mouseenter", () => fill.setAttribute("data-state", "hover"));
  row.addEventListener("mouseleave", () => fill.removeAttribute("data-state"));
}

function actionRow(icon, label, onClick) {
  const row = document.createElement("div");
  row.className = "sidebar-action";
  row.setAttribute("role", "button");
  row.setAttribute("aria-label", label);
  row.innerHTML = `<span class="flat-fill" data-penna="row.nav"></span><i data-icon="${icon}" class="flat-glyph"></i><span data-text></span>`;
  row.querySelector("[data-text]").textContent = label;
  hoverFill(row);
  row.addEventListener("click", onClick);
  return row;
}

function renderActions() {
  const rows = preferences.flat_sidebar && state.connected ? [actionRow("chat-plus", "New chat", () => {
    if (!writable()) return;
    state.client.new_chat();
    markTree();
  })] : [];
  actions.replaceChildren(...rows);
}

export { paneIcon };

export function select(entry, window, pane) {
  state.client.sidebar_activate(pane ? `pane-%${pane.id}` : window ? `window-@${window.id}` : `session-$${entry.id}`);
  flush();
}

export function toggleSidebar() {
  if (narrow()) {
    if (slideover) release();
    else focusSidebar();
    return;
  }
  slideover = false;
  setPreferences({ sidebar: !preferences.sidebar });
}

function toggleSidebarKey() {
  if (slideover) release();
  else if (!preferences.sidebar) focusSidebar();
  else toggleSidebar();
}

function focusSidebar() {
  slideover = !sidebarInline();
  state.client?.sidebar_focus(true);
  applyLayout();
  nav.focus({ preventScroll: true });
  markTree();
}

function release() {
  slideover = false;
  state.client?.sidebar_focus(false);
  applyLayout();
  emit("focusTerminal");
  document.querySelector(".ime")?.focus({ preventScroll: true });
  markTree();
}

function marker(icon, row) {
  const element = document.createElement("span");
  element.className = "tree-marker";
  element.innerHTML = row.node === "host" ? hostMark(row.platform) : programMaskHtml(row.program, icon);
  if (row.bell) element.insertAdjacentHTML("beforeend", `<i class="marker-dot bell"></i>`);
  if (row.badge) element.insertAdjacentHTML("beforeend", `<i class="marker-dot badge" data-badge="${row.badge}"></i>`);
  return element;
}

function action(icon, title, run) {
  const button = document.createElement("button");
  button.dataset.penna = "icon-button.row";
  button.dataset.inside = "";
  button.className = "tree-action";
  button.title = title;
  button.disabled = !writable();
  button.innerHTML = `<i data-icon="${icon}"></i>`;
  button.addEventListener("pointerdown", (event) => event.stopPropagation());
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    run();
    flush();
  });
  return button;
}

function treeRow(row, rows, entry) {
  const isActiveHost = !entry || entry.id === state.hostId;
  const hostClient = entry ? entry.client : state.client;
  const live = entry ? entry.connected || (isActiveHost && state.connected) : state.connected;
  const element = document.createElement("div");
  element.dataset.penna = "sidebar.row";
  element.dataset.key = row.key;
  element.dataset.node = row.node;
  element.style.setProperty("--depth", row.depth);
  element.toggleAttribute("data-path", live && isActiveHost && row.onActivePath);
  element.toggleAttribute("data-host", row.node === "host");
  element.toggleAttribute("data-active", isActiveHost && row.active);
  element.toggleAttribute("data-expandable", row.expandable);
  if (isActiveHost && (row.selected || (row.active && rows.focused))) element.dataset.state = "current";
  const icon = NODE_ICONS[row.node] || paneIcon(row.node);
  const slot = marker(icon, row);
  if (row.expandable) {
    const disclosure = document.createElement("span");
    disclosure.className = "tree-disclosure";
    disclosure.append(slot);
    disclosure.insertAdjacentHTML("beforeend", `<i data-icon="chevron-right" class="chevron"${row.expanded ? " data-open" : ""}></i>`);
    disclosure.addEventListener("pointerdown", (event) => event.stopPropagation());
    disclosure.addEventListener("click", (event) => {
      event.stopPropagation();
      hostClient.sidebar_toggle(row.key, true);
      markTree();
    });
    element.append(disclosure);
  } else {
    element.append(slot);
  }
  const label = document.createElement("span");
  label.className = "penna-label";
  label.innerHTML = `<span class="tree-name" data-text></span>`;
  label.firstChild.textContent = row.label;
  if (row.index) {
    const index = document.createElement("span");
    index.className = "tree-index";
    index.dataset.text = "";
    index.textContent = row.index;
    label.append(index);
  }
  element.append(label);
  const actions = document.createElement("span");
  actions.className = "tree-actions";
  if (row.node === "host" && !live) {
    actions.insertAdjacentHTML("beforeend", `<i data-icon="triangle-alert" class="host-alert"></i>`);
  }
  if (row.add) actions.append(action(row.node === "host" ? "layers-plus" : "window-plus", row.add, () => switchHost(entry.id, (client) => client.sidebar_command(row.key, "add"))));
  if (row.close) actions.append(action("xmark", row.close, () => switchHost(entry.id, (client) => client.sidebar_command(row.key, "close"))));
  element.append(actions);
  element.addEventListener("pointerdown", () => {
    if (!focused()) {
      hostClient.sidebar_focus(true);
      nav.focus({ preventScroll: true });
    }
  });
  element.addEventListener("click", () => {
    hostClient.sidebar_select(row.key, true);
    if (row.node === "host" && live) {
      hostClient.sidebar_toggle(row.key, true);
      markTree();
      return;
    }
    if (slideover && row.node !== "host" && live) release();
    if (live) {
      if (entry && !isActiveHost) {
        switchHost(entry.id, (client) => client.sidebar_activate(row.key));
      } else {
        hostClient.sidebar_activate(row.key);
        flush();
      }
    }
    markTree();
  });
  if (row.rename && writable()) {
    element.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      contextMenu(event.clientX, event.clientY, [[row.rename, () => switchHost(entry.id, (client) => client.sidebar_command(row.key, "rename"))]]);
    });
  }
  return element;
}

function drawGuides(rows) {
  const runs = [];
  const depths = rows.map((row) => row.depth);
  const activeRow = rows.findLastIndex((row) => row.onActivePath);
  const activeDepth = activeRow >= 0 ? depths[activeRow] : -1;
  const deepest = Math.max(0, ...depths);
  for (let level = 0; level < deepest; level++) {
    let start = -1;
    for (let index = 0; index <= depths.length; index++) {
      const inside = index < depths.length && depths[index] > level;
      if (inside && start < 0) start = index;
      if (!inside && start >= 0) {
        const active = state.connected && level + 1 === activeDepth && start <= activeRow && activeRow < index;
        runs.push({ level, start, count: index - start, active });
        start = -1;
      }
    }
  }
  guides.replaceChildren(
    ...runs.map((run) => {
      const line = document.createElement("i");
      line.className = run.active ? "guide active" : "guide";
      line.style.left = `${run.level * 20 + 24}px`;
      line.style.top = `${run.start * 32 + 4}px`;
      line.style.height = `${run.count * 32 - 8}px`;
      return line;
    }),
  );
}

function renderTree() {
  const entries = state.hosts.length ? state.hosts : state.client ? [{ id: state.hostId, client: state.client, connected: state.connected }] : [];
  const allRows = [];
  const elements = [];
  let scrollTarget = null;
  for (const entry of entries) {
    const isActive = entry.id === state.hostId;
    const model = JSON.parse(entry.client.sidebar_rows(isActive && document.hasFocus()));
    if (isActive && model.scroll !== null && model.scroll !== undefined) {
      scrollTarget = allRows.length + model.scroll;
    }
    for (const row of model.rows) {
      allRows.push({ ...row, onActivePath: isActive && row.onActivePath });
      elements.push(treeRow(row, model, entry));
    }
  }
  list.className = "tree";
  list.replaceChildren(guides, ...elements);
  drawGuides(allRows);
  if (scrollTarget !== null) list.children[scrollTarget + 1]?.scrollIntoView({ block: "nearest" });
}

function rollup(counts, live) {
  const element = document.createElement("span");
  element.className = "flat-rollup";
  if (live && counts.working) element.insertAdjacentHTML("beforeend", `<span><i data-icon="loader" class="spin"></i>${counts.working}</span>`);
  for (const tone of ["warning", "danger", "success"]) {
    if (counts[tone]) element.insertAdjacentHTML("beforeend", `<span><i class="dot ${tone}"></i>${counts[tone]}</span>`);
  }
  return element;
}

function tileStack(tiles, onPick) {
  const stack = document.createElement("span");
  stack.className = "flat-stack";
  tiles.slice(0, 3).forEach((tile, index) => {
    const element = document.createElement("span");
    element.dataset.penna = "tile.stack";
    element.className = "flat-tile";
    if (index === 0) element.dataset.state = "current";
    element.style.left = `${index * 7}px`;
    element.style.zIndex = 3 - index;
    paintTile(element, tile);
    element.title = tile.label;
    if (onPick) {
      element.addEventListener("click", (event) => {
        event.stopPropagation();
        onPick(tile);
      });
    }
    stack.append(element);
  });
  return stack;
}

function attachHover(row, fill) {
  row.addEventListener("mouseenter", () => fill.setAttribute("data-state", `${fill.dataset.state || ""} hover`.trim()));
  row.addEventListener("mouseleave", () => {
    const states = (fill.dataset.state || "").split(" ").filter((item) => item && item !== "hover");
    if (states.length) fill.dataset.state = states.join(" ");
    else delete fill.dataset.state;
  });
}

function detailText(titleText, parts, ageText, working, live) {
  const text = document.createElement("span");
  text.className = "flat-text";
  text.innerHTML = `<span class="flat-title"><span class="flat-label"></span></span><span class="flat-detail"><span class="flat-parts"></span></span>`;
  text.querySelector(".flat-label").textContent = titleText;
  text.querySelector(".flat-label").setAttribute("aria-label", titleText);
  text.querySelector(".flat-parts").setAttribute("aria-label", parts.map((part) => part.text).join(""));
  if (working && live) text.querySelector(".flat-title").insertAdjacentHTML("beforeend", `<i data-icon="loader" class="spin"></i>`);
  for (const part of parts) {
    const span = document.createElement("span");
    span.dataset.tone = part.tone;
    span.textContent = part.text;
    text.querySelector(".flat-parts").append(span);
  }
  if (ageText) {
    const age = document.createElement("span");
    age.className = "flat-age";
    age.textContent = ageText;
    text.querySelector(".flat-detail").append(age);
  }
  return text;
}

function flatPaneRow(entry, windowId, pane, isHostActive, live) {
  const row = document.createElement("div");
  row.className = "flat-pane";
  const fill = document.createElement("span");
  fill.className = "flat-fill";
  fill.dataset.penna = "row.nav";
  if (isHostActive && pane.current) fill.dataset.state = "current";
  row.append(fill);
  attachHover(row, fill);
  row.setAttribute("role", "button");
  row.setAttribute("aria-label", pane.title);
  row.setAttribute("aria-description", pane.description);
  row.toggleAttribute("data-dimmed", !live);
  const badge = document.createElement("span");
  badge.dataset.penna = "tile.stack";
  badge.className = "flat-pane-tile";
  paintTile(badge, pane.tile);
  const label = document.createElement("span");
  label.className = "flat-pane-label";
  label.textContent = pane.title;
  row.append(badge, label);
  if (pane.working && live) row.insertAdjacentHTML("beforeend", `<i data-icon="loader" class="spin"></i>`);
  row.addEventListener("click", () => {
    if (!live) return;
    if (slideover) release();
    switchHost(entry.id, (client) => {
      client.sidebar_activate(`window-@${windowId}`);
      client.select_pane(pane.pane, false);
    });
  });
  return row;
}

function flatWindowRow(entry, win, stackWidth, isHostActive, live, nested = false) {
  const row = document.createElement("div");
  row.className = nested ? "flat-window nested" : "flat-window";
  const fill = document.createElement("span");
  fill.className = "flat-fill";
  fill.dataset.penna = "row.nav";
  if (isHostActive && win.current) fill.dataset.state = "current";
  row.append(fill);
  attachHover(row, fill);
  row.setAttribute("role", "button");
  row.setAttribute("aria-label", win.title);
  row.setAttribute("aria-description", win.description);
  row.toggleAttribute("data-dimmed", !live);
  const stack = tileStack(win.tiles, (tile) => {
    if (!live) return;
    if (slideover) release();
    switchHost(entry.id, (client) => {
      client.select_pane(tile.pane, true);
    });
  });
  stack.style.width = `${stackWidth}px`;
  const text = detailText(win.title, win.detail, win.age, win.working, live);
  row.append(stack, text);
  if (win.opens && !nested) {
    const toggle = document.createElement("button");
    toggle.className = "flat-expand";
    toggle.dataset.penna = "icon-button.row";
    toggle.title = win.open ? "Collapse panes" : "Show panes";
    toggle.innerHTML = `<i data-icon="chevron-right" class="chevron"${win.open ? " data-open" : ""}></i>`;
    toggle.addEventListener("click", (event) => {
      event.stopPropagation();
      entry.client.flat_toggle_window(win.id);
      markTree();
    });
    row.append(toggle);
  }
  row.addEventListener("click", () => {
    if (!live) return;
    if (slideover) release();
    switchHost(entry.id, (client) => {
      client.sidebar_activate(`window-@${win.id}`);
    });
  });
  row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    if (!live) return;
    contextMenu(event.clientX, event.clientY, [
      ["Rename window", () => switchHost(entry.id, (client) => client.sidebar_command(`window-@${win.id}`, "rename"))],
      ["Close window", () => switchHost(entry.id, (client) => client.sidebar_command(`window-@${win.id}`, "close"))],
    ]);
  });
  const nodes = [row];
  if (win.open && win.panes?.length) {
    const group = document.createElement("div");
    group.className = "flat-children";
    for (const pane of win.panes) {
      group.append(flatPaneRow(entry, win.id, pane, isHostActive, live));
    }
    nodes.push(group);
  }
  return nodes;
}

function flatPileRow(entry, pile, stackWidth, isHostActive, live) {
  const row = document.createElement("div");
  row.className = "flat-window";
  const fill = document.createElement("span");
  fill.className = "flat-fill";
  fill.dataset.penna = "row.nav";
  if (isHostActive && pile.current) fill.dataset.state = "current";
  row.append(fill);
  attachHover(row, fill);
  row.setAttribute("role", "button");
  row.setAttribute("aria-label", pile.title);
  row.setAttribute("aria-description", pile.description);
  row.toggleAttribute("data-dimmed", !live);
  const stack = tileStack(pile.tiles, (tile) => {
    if (!live) return;
    if (slideover) release();
    switchHost(entry.id, (client) => {
      client.select_pane(tile.pane, true);
    });
  });
  stack.style.width = `${stackWidth}px`;
  const text = detailText(pile.title, pile.detail, pile.age, pile.working, live);
  const toggle = document.createElement("button");
  toggle.className = "flat-expand";
  toggle.dataset.penna = "icon-button.row";
  toggle.title = pile.open ? "Collapse windows" : "Show windows";
  toggle.innerHTML = `<i data-icon="chevron-right" class="chevron"${pile.open ? " data-open" : ""}></i>`;
  toggle.addEventListener("click", (event) => {
    event.stopPropagation();
    entry.client.flat_toggle_pile();
    markTree();
  });
  row.append(stack, text, toggle);
  row.addEventListener("click", () => {
    if (!live) return;
    if (slideover) release();
    switchHost(entry.id, (client) => {
      client.sidebar_activate(`window-@${pile.id}`);
    });
  });
  const nodes = [row];
  if (pile.open && pile.windows?.length) {
    const group = document.createElement("div");
    group.className = "flat-children";
    for (const win of pile.windows) {
      group.append(...flatWindowRow(entry, win, 28, isHostActive, live, true));
    }
    nodes.push(group);
  }
  return nodes;
}

function renderFlat() {
  const now = Date.now() / 1000;
  const entries = state.hosts.length ? state.hosts : state.client ? [{ id: state.hostId, client: state.client, connected: state.connected }] : [];
  const models = entries.map((entry) => ({
    entry,
    host: JSON.parse(entry.client.flat_sidebar(now)),
    live: entry.connected || (entry.id === state.hostId && state.connected),
  }));
  const allRows = models.flatMap(({ host }) => host.rows || []);
  const maxTiles = Math.min(Math.max(1, ...allRows.map((row) => Math.min(row.tiles?.length || 1, 3))), 3);
  const stackWidth = 28 + (maxTiles > 1 ? (maxTiles - 1) * 7 : 0);
  const nodes = [];
  models.forEach(({ entry, host, live }, hostIndex) => {
    const rows = host.rows || [];
    if (!rows.length && models.length === 1) return;
    const folded = Boolean(flatFolds[entry.id]);
    const header = document.createElement("div");
    header.className = hostIndex > 0 ? "flat-host separated" : "flat-host";
    header.setAttribute("role", "button");
    header.setAttribute("aria-label", host.name);
    header.setAttribute("aria-expanded", String(!folded));
    header.innerHTML = `<i data-icon="${host.platform ? `host-${host.platform}` : "laptop"}" class="flat-glyph"></i><span class="flat-name" data-text></span><i data-icon="chevron-right" class="chevron"${folded ? "" : " data-open"}></i><span class="grow"></span>`;
    header.querySelector(".flat-name").textContent = host.name;
    if (folded) header.append(rollup(host.rollup, live));
    if (!live) header.insertAdjacentHTML("beforeend", `<i data-icon="circle-x" class="flat-status"></i>`);
    header.addEventListener("click", () => {
      flatFolds[entry.id] = !folded;
      markTree();
    });
    nodes.push(header);
    if (!folded) {
      const isHostActive = entry.id === state.hostId;
      for (const item of rows) {
        if (item.kind === "pile") nodes.push(...flatPileRow(entry, item, stackWidth, isHostActive, live));
        else nodes.push(...flatWindowRow(entry, item, stackWidth, isHostActive, live));
      }
    }
  });
  list.className = "flat";
  list.replaceChildren(...nodes);
}

function fade() {
  list.toggleAttribute("data-fade", list.scrollTop + list.clientHeight < list.scrollHeight - 1);
}

function render() {
  applyLayout();
  if (!state.client) return;
  renderActions();
  if (preferences.flat_sidebar) renderFlat();
  else renderTree();
  fade();
}

list.addEventListener("scroll", fade, { passive: true });

phase("tree", render);

toggleButton.addEventListener("click", toggleSidebar);
appMenuButton?.addEventListener("click", (event) => {
  event.stopPropagation();
  openAppMenu(appMenuButton);
});
hoverFill($("#open-settings"));
viewButton.addEventListener("click", () => setPreferences({ flat_sidebar: !preferences.flat_sidebar }));
on("theme", applyLayout);
scrim.addEventListener("pointerdown", (event) => {
  event.preventDefault();
  release();
});
on("toggleSidebar", toggleSidebar);
on("focusSidebar", focusSidebar);
on("preferences", applyLayout);
on("hello", () => state.client.sidebar_hello(Date.now() / 1000));
on("agentState", (event) => {
  if (event.attention === "done") {
    state.client.sidebar_agent_done(event.pane, document.hasFocus());
    markTree();
  }
});

nav.addEventListener("focusout", () => {
  setTimeout(() => {
    if (focused()) return;
    state.client?.sidebar_focus(false);
    if (slideover) release();
    else markTree();
  });
});

handle.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  event.preventDefault();
  try {
    handle.setPointerCapture(event.pointerId);
  } catch {}
  const left = nav.getBoundingClientRect().left;
  const move = (moveEvent) => {
    const next = Math.min(Math.max(moveEvent.clientX - left, 160), Math.min(Math.max(innerWidth * 0.5, 160), 640));
    if (next !== preferences.sidebar_width) {
      preferences.sidebar_width = next;
      applyLayout();
    }
  };
  const up = () => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", up);
    setPreferences({ sidebar_width: preferences.sidebar_width });
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", up);
});

addEventListener("resize", applyLayout);

onKey((event) => {
  if (!state.client || event.isComposing) return false;
  if (event.target.closest?.("input, textarea, select") && !event.target.classList.contains("ime") && !nav.contains(event.target)) return false;
  const resolve = (table) => state.client.workspace_key(table, event.key, event.shiftKey, event.ctrlKey, event.altKey, event.metaKey);
  if (focused()) {
    const action = preferences.flat_sidebar ? null : resolve("sidebar");
    if (action === "prefix") return false;
    if (action && sidebarAction(action)) {
      event.preventDefault();
      return true;
    }
  }
  const action = resolve("ui");
  if (action === "prefix") return false;
  if (action === "toggle-sidebar") {
    event.preventDefault();
    toggleSidebarKey();
    return true;
  }
  return focused() && !event.metaKey && !["Shift", "Control", "Alt", "Meta"].includes(event.key);
});

function sidebarAction(action) {
  if (action.startsWith("sidebar-select-")) {
    state.client.sidebar_key(action, document.hasFocus());
    markTree();
    requestAnimationFrame(() => list.querySelector("[data-state~='current']")?.scrollIntoView({ block: "nearest" }));
    return true;
  }
  const selected = state.client.sidebar_key(action, document.hasFocus());
  if (action === "sidebar-cancel") release();
  else if (action === "toggle-sidebar") toggleSidebarKey();
  else if (action === "sidebar-confirm" && selected) {
    if (selected === "host" && state.connected) {
      state.client.sidebar_toggle(selected, false);
      markTree();
    } else {
      state.client.sidebar_activate(selected);
      release();
    }
  } else if (action === "sidebar-rename" && selected && writable()) {
    state.client.sidebar_command(selected, "rename");
  } else if (action === "sidebar-command-palette") {
    emit("openPalette", { mode: "command" });
  } else return false;
  flush();
  return true;
}

on("status", ({ text }) => {
  $("#status").textContent = text;
  $("#status").style.visibility = "hidden";
});

on("prefix", ({ armed }) => {
  $("#prefix").hidden = !armed;
});
