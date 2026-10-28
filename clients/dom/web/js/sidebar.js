import { $, emit, flush, markTree, on, onKey, phase, state } from "./core.js";
import { contextMenu, narrow, paneIcon, preferences, setPreferences, sidebarInline } from "./workspace.js";

const app = $(".app");
const nav = $("nav[data-zz='sidebar']");
const list = $("#tree");
const toggleButton = $("#toggle-sidebar");
const viewButton = $("#flat-list");
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
let flatFolds = { host: false };

const NODE_ICONS = { host: "hard-drive", session: "layers", window: "app-window" };

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
  app.style.setProperty("--zz-sidebar-width", inline ? `${width()}px` : "0px");
  nav.style.width = `${width()}px`;
  nav.toggleAttribute("data-slideover", !inline && slideover);
  nav.hidden = !inline && !slideover;
  nav.toggleAttribute("data-divider", !(preferences.gaps && inline));
  scrim.hidden = inline || !slideover;
  viewButton.title = preferences.flat_sidebar ? "Show the session tree" : "Show chats";
  viewButton.querySelector("[data-icon]").dataset.icon = preferences.flat_sidebar ? "list" : "list-tree";
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
  element.innerHTML = row.node === "host" ? `<i class="host-logo"></i>` : `<i data-icon="${icon}"></i>`;
  if (row.bell) element.insertAdjacentHTML("beforeend", `<i class="marker-dot bell"></i>`);
  if (row.badge) element.insertAdjacentHTML("beforeend", `<i class="marker-dot badge" data-badge="${row.badge}"></i>`);
  return element;
}

function action(icon, title, run) {
  const button = document.createElement("button");
  button.dataset.zz = "sidebar.row-action";
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

function treeRow(row, rows) {
  const element = document.createElement("div");
  element.dataset.zz = "sidebar.row";
  element.dataset.key = row.key;
  element.dataset.node = row.node;
  element.style.setProperty("--depth", row.depth);
  element.toggleAttribute("data-path", state.connected && row.onActivePath);
  element.toggleAttribute("data-host", row.node === "host");
  element.toggleAttribute("data-active", row.active);
  element.toggleAttribute("data-expandable", row.expandable);
  if (row.selected || (row.active && rows.focused)) element.dataset.state = "current";
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
      state.client.sidebar_toggle(row.key, true);
      markTree();
    });
    element.append(disclosure);
  } else {
    element.append(slot);
  }
  const label = document.createElement("span");
  label.className = "zz-label";
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
  if (row.node === "host" && !state.connected) {
    actions.insertAdjacentHTML("beforeend", `<i data-icon="triangle-alert" class="host-alert"></i>`);
  }
  if (row.add) actions.append(action(row.node === "host" ? "layers-plus" : "window-plus", row.add, () => state.client.sidebar_command(row.key, "add")));
  if (row.close) actions.append(action("xmark", row.close, () => state.client.sidebar_command(row.key, "close")));
  element.append(actions);
  element.addEventListener("pointerdown", () => {
    if (!focused()) {
      state.client.sidebar_focus(true);
      nav.focus({ preventScroll: true });
    }
  });
  element.addEventListener("click", () => {
    state.client.sidebar_select(row.key, true);
    if (row.node === "host" && state.connected) {
      state.client.sidebar_toggle(row.key, true);
    } else if (slideover && row.node !== "host" && state.connected) {
      release();
    }
    if (state.connected) state.client.sidebar_activate(row.key);
    flush();
    markTree();
  });
  if (row.rename && writable()) {
    element.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      contextMenu(event.clientX, event.clientY, [[row.rename, () => state.client.sidebar_command(row.key, "rename")]]);
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
      line.style.left = `${run.level * 20 + 17}px`;
      line.style.top = `${run.start * 32 + 4}px`;
      line.style.height = `${run.count * 32 - 8}px`;
      return line;
    }),
  );
}

function renderTree() {
  const model = JSON.parse(state.client.sidebar_rows(document.hasFocus()));
  list.className = "tree";
  list.replaceChildren(guides, ...model.rows.map((row) => treeRow(row, model)));
  drawGuides(model.rows);
  if (model.scroll !== null && model.scroll !== undefined) list.children[model.scroll + 1]?.scrollIntoView({ block: "nearest" });
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

function addButton(icon, title, run) {
  const button = document.createElement("button");
  button.dataset.zz = "sidebar.row-action";
  button.className = "flat-add";
  button.title = title;
  button.innerHTML = `<i data-icon="${icon}"></i>`;
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    if (writable()) run();
    flush();
  });
  return button;
}

function tileStack(window) {
  const stack = document.createElement("span");
  stack.className = "flat-stack";
  window.tiles.slice(0, 3).forEach((tile, index) => {
    const element = document.createElement("span");
    element.dataset.zz = "pane-stack";
    element.className = "flat-tile";
    if (index === 0) element.dataset.state = "current";
    element.style.left = `${index * 7}px`;
    element.style.zIndex = 3 - index;
    element.innerHTML = `<i data-icon="${tile.tag === "terminal" ? "terminal" : paneIcon(tile.tag)}"></i>`;
    element.title = tile.label;
    element.addEventListener("click", (event) => {
      event.stopPropagation();
      if (!state.connected) return;
      state.client.select_pane(tile.pane, true);
      flush();
    });
    stack.append(element);
  });
  return stack;
}

function renderFlat() {
  const host = JSON.parse(state.client.flat_sidebar(Date.now() / 1000));
  const live = state.connected;
  const rows = [];
  const header = document.createElement("div");
  header.className = "flat-host";
  header.setAttribute("role", "button");
  header.setAttribute("aria-label", host.name);
  header.setAttribute("aria-expanded", String(!flatFolds.host));
  header.innerHTML = `<i data-icon="laptop" class="flat-glyph"></i><span class="flat-name" data-text></span><i data-icon="chevron-right" class="chevron"${flatFolds.host ? "" : " data-open"}></i><span class="grow"></span>`;
  header.querySelector(".flat-name").textContent = host.name;
  if (flatFolds.host) header.append(rollup(host.rollup, live));
  if (!live) header.insertAdjacentHTML("beforeend", `<i data-icon="circle-x" class="flat-status"></i>`);
  if (live) header.append(addButton("chat-plus", "New agent", () => state.client.new_chat()));
  header.addEventListener("click", () => {
    flatFolds.host = !flatFolds.host;
    markTree();
  });
  rows.push(header);
  if (!flatFolds.host) {
    const tiles = Math.max(0, ...host.chats.map((window) => window.tiles.length));
    for (const window of host.chats) {
      const row = document.createElement("div");
      row.className = "flat-window";
      const fill = document.createElement("span");
      fill.className = "flat-fill";
      fill.dataset.zz = "sidebar.entry";
      row.append(fill);
      row.addEventListener("mouseenter", () => fill.setAttribute("data-state", `${fill.dataset.state || ""} hover`.trim()));
      row.addEventListener("mouseleave", () => {
        const states = (fill.dataset.state || "").split(" ").filter((item) => item && item !== "hover");
        if (states.length) fill.dataset.state = states.join(" ");
        else delete fill.dataset.state;
      });
      row.setAttribute("role", "button");
      row.setAttribute("aria-label", window.title);
      row.setAttribute("aria-description", window.description);
      if (window.current) fill.dataset.state = "current";
      row.toggleAttribute("data-dimmed", !live);
      const text = document.createElement("span");
      text.className = "flat-text";
      text.innerHTML = `<span class="flat-title"><span class="flat-label"></span></span><span class="flat-detail"><span class="flat-parts"></span></span>`;
      text.querySelector(".flat-label").textContent = window.title;
      text.querySelector(".flat-label").setAttribute("aria-label", window.title);
      text.querySelector(".flat-parts").setAttribute("aria-label", window.detail.map((part) => part.text).join(""));
      if (window.working && live) text.querySelector(".flat-title").insertAdjacentHTML("beforeend", `<i data-icon="loader" class="spin"></i>`);
      for (const part of window.detail) {
        const span = document.createElement("span");
        span.dataset.tone = part.tone;
        span.textContent = part.text;
        text.querySelector(".flat-parts").append(span);
      }
      if (window.age) {
        const age = document.createElement("span");
        age.className = "flat-age";
        age.textContent = window.age;
        text.querySelector(".flat-detail").append(age);
      }
      const stack = tileStack(window);
      stack.style.width = `${28 + (Math.max(tiles, window.tiles.length, 1) > 1 ? (Math.min(Math.max(tiles, 1), 3) - 1) * 7 : 0)}px`;
      row.append(stack, text);
      row.addEventListener("click", () => {
        if (slideover && state.connected) release();
        state.client.sidebar_activate(`window-@${window.id}`);
        flush();
      });
      row.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        if (!writable()) return;
        contextMenu(event.clientX, event.clientY, [
          ["Rename window", () => state.client.sidebar_command(`window-@${window.id}`, "rename")],
          ["Close window", () => state.client.sidebar_command(`window-@${window.id}`, "close")],
        ]);
      });
      rows.push(row);
    }
  }
  list.className = "flat";
  list.replaceChildren(...rows);
}

function fade() {
  list.toggleAttribute("data-fade", list.scrollTop + list.clientHeight < list.scrollHeight - 1);
}

function render() {
  applyLayout();
  if (!state.client) return;
  if (preferences.flat_sidebar) renderFlat();
  else renderTree();
  fade();
}

list.addEventListener("scroll", fade, { passive: true });

phase("tree", render);

toggleButton.addEventListener("click", toggleSidebar);
viewButton.addEventListener("click", () => setPreferences({ flat_sidebar: !preferences.flat_sidebar }));
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
