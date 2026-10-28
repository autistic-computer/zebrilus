import { $, activeWindow, emit, flush, markPane, markTree, on, onKey, phase, schedule, session, state } from "./core.js";
import { preferences, setPreferences } from "./preferences.js";

export { preferences, setPreferences };

export const panes = new Map();
const renderers = new Map();
const headerButtons = [];

export function paneRenderer(kind, renderer) {
  renderers.set(kind, renderer);
}

export function paneHeaderButton(factory) {
  headerButtons.push(factory);
}

export function paneIcon(kind) {
  return { browser: "globe", editor: "file", picker: "plus" }[kind] || "square-terminal";
}

export function metrics() {
  const margin = preferences.gaps ? preferences.pane_margin : 0;
  const header = margin > 0 ? 34 : 40;
  return { margin, header, titlebar: header + margin * 2, gaps: preferences.gaps };
}

const app = $(".app");
const workspace = $("#workspace");
const titlebar = document.createElement("div");
titlebar.id = "titlebar";
titlebar.dataset.zz = "titlebar";
const canvas = document.createElement("div");
canvas.id = "canvas";
const content = document.createElement("div");
content.className = "canvas-content";
const floatLayer = document.createElement("div");
floatLayer.className = "float-layer";
const preview = document.createElement("div");
preview.className = "drop-preview";
preview.dataset.zz = "pane.drop-zone";
preview.dataset.state = "selected";
const toast = document.createElement("div");
toast.className = "canvas-toast";
toast.dataset.zz = "toast";
toast.hidden = true;
const empty = document.createElement("div");
empty.className = "empty-workspace";
empty.hidden = true;
empty.innerHTML = `<i data-icon="square-terminal"></i><div class="empty-title" data-text>Your workspace</div><div class="empty-status zz-muted" data-text></div><button data-zz="button"></button>`;
canvas.append(content, preview, toast, empty);
workspace.append(titlebar, canvas);

const chip = document.createElement("div");
chip.className = "drag-chip";
chip.dataset.zz = "pane.drag-chip";
chip.hidden = true;
chip.innerHTML = `<b class="zz-mono"></b><span class="zz-muted"></span>`;
document.body.append(chip);

let prefixArmed = false;
let arrived = { window: null, known: new Set() };
let override = null;
let splitDrag = null;
let paneDrag = null;
let pipDrag = null;
const pickers = new Map();

function writable() {
  return state.connected && !state.tree?.readOnly;
}

function canDrag(window) {
  return writable() && !window.zoomedPane && window.panes.length > 1;
}

function el(tag, attrs = {}, html = "") {
  const element = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key === "class") element.className = value;
    else if (key.startsWith("data-") || key === "title" || key === "aria-label") element.setAttribute(key, value);
    else element[key] = value;
  }
  if (html) element.innerHTML = html;
  return element;
}

function iconButton(role, action, icon, label) {
  const button = el("button", { "data-zz": role, "data-action": action, title: label });
  button.innerHTML = `<i data-icon="${icon}"></i>`;
  return button;
}

function headerActions(floats) {
  return floats
    ? [
        ["tile", "layout-columns", "Tile back"],
        ["zoom", "window-maximize", "Zoom"],
      ]
    : [
        ["split-bottom", "panel-bottom", "Split bottom"],
        ["split-right", "panel-right", "Split right"],
        ["float", "picture-in-picture", "Float"],
      ];
}

function header(pane) {
  const element = el("div", { "data-zz": "pane.header" });
  const slot = el("div", { class: "title-slot" });
  const title = el("div", { "data-zz": "pane.header.title", class: "pane-title" });
  title.innerHTML = `<span class="title-icon"><i data-icon="square-terminal" class="kind"></i><i data-icon="arrows-move" class="move"></i></span><span class="title" data-text></span>`;
  const close = iconButton("pane.header.button", "close", "xmark", "Close pane");
  close.classList.add("title-close");
  title.append(close);
  slot.append(title);
  const extras = el("div", { class: "header-extras" });
  const actions = el("div", { class: "header-actions" });
  element.append(slot, extras, actions);
  element.addEventListener("click", (event) => {
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (!action) {
      focusPane(pane.id);
      return;
    }
    event.stopPropagation();
    runAction(pane.id, action);
  });
  title.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("[data-action]")) return;
    if (pane.floating) beginPip(pane, event);
    else beginPaneDrag(pane.id, event, false);
  });
  return { element, title, extras, actions, floats: null };
}

function runAction(id, action) {
  if (!writable()) return;
  const rows = action === "float" ? floatRows() : 0;
  state.client.pane_action(id, action, rows);
  flush();
}

export function focusPane(id) {
  const window = activeWindow();
  if (!window || !state.client) return;
  state.client.focus_pane(id);
  flush();
}

function paneElement(id) {
  let pane = panes.get(id);
  if (pane) return pane;
  const element = el("section", { "data-zz": "pane", "data-pane": id });
  const frame = el("div", { class: "pane-header-frame" });
  const body = el("div", { class: "pane-body" });
  const scrim = el("div", { class: "pane-scrim" });
  const badges = el("div", { class: "pane-badges" });
  const veil = el("div", { class: "pane-veil" });
  element.append(frame, body, scrim, badges, veil);
  pane = { id, element, frame, body, scrim, badges, veil, header: null, kind: null, view: null, info: null };
  element.addEventListener(
    "pointerdown",
    () => {
      const window = activeWindow();
      if (window && window.activePane !== id) focusPane(id);
    },
    true,
  );
  element.addEventListener("mousemove", (event) => {
    const window = activeWindow();
    if (state.tree?.focusFollowsMouse && !event.buttons && window && window.activePane !== id) {
      state.client.pane_action(id, "select", 0);
      flush();
    }
  });
  veil.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    beginPaneDrag(id, event, true);
  });
  badges.addEventListener("pointerdown", (event) => event.stopPropagation());
  badges.addEventListener("click", (event) => {
    if (event.target.closest("[data-action='unzoom']") && writable()) {
      state.client.pane_action(id, "zoom", 0);
      flush();
    }
  });
  panes.set(id, pane);
  return pane;
}

function mount(pane, info) {
  const kind = info?.kind || "terminal";
  if (pane.kind === kind) return;
  pane.view?.destroy?.();
  pane.kind = kind;
  pane.body.replaceChildren();
  const renderer = renderers.get(kind) || renderers.get("terminal");
  pane.view = renderer ? renderer(pane) : null;
}

function badge(icon, label) {
  return `<div data-zz="pane.chip" class="pane-badge"><i data-icon="${icon}"></i><span data-text>${escape(label)}</span></div>`;
}

function escape(text) {
  return String(text).replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[char]);
}

function chrome(pane, info, window, floating) {
  const active = window.activePane === pane.id;
  const terminal = info.kind === "terminal";
  const lens = pane.element.hasAttribute("data-lens");
  const { header: headerHeight, gaps } = metrics();
  const framed = gaps && !floating;
  const showHeader = terminal && !floating;
  pane.info = info;
  pane.floating = floating;
  pane.element.toggleAttribute("data-active", active);
  pane.element.toggleAttribute("data-framed", framed);
  pane.element.toggleAttribute("data-floating", floating);
  pane.element.dataset.kind = info.kind;
  const opacity = preferences.pane_inactive_opacity;
  pane.element.style.setProperty("--pane-dim", active ? 1 : opacity);
  pane.element.toggleAttribute("data-dim-text", !active && terminal && !lens);
  pane.element.toggleAttribute("data-dim-scrim", !active && (lens || !terminal) && opacity < 1);
  if (showHeader) {
    if (!pane.header) {
      pane.header = header(pane);
      pane.frame.replaceChildren(pane.header.element);
    }
    renderHeader(pane.header, pane, info, window, false);
  }
  pane.frame.hidden = !showHeader;
  const tags = [];
  if (terminal && info.waiting && !lens) tags.push(badge("clock", `Waiting for %${pane.id}`));
  if (info.deadLabel) tags.push(badge("circle-x", info.deadLabel));
  if (info.synchronized) tags.push(badge("layers", "Sync"));
  if (window.zoomedPane === pane.id) {
    tags.push(`<button data-zz="pane.chip" class="unzoom" data-action="unzoom" aria-label="Unzoom"><i data-icon="window-restore"></i><span data-text>Unzoom</span></button>`);
  }
  const html = tags.join("");
  if (pane.badges.dataset.html !== html) {
    pane.badges.dataset.html = html;
    pane.badges.innerHTML = html;
  }
  pane.badges.style.top = `${(showHeader ? headerHeight : 0) + 8}px`;
  pane.badges.hidden = !tags.length;
}

function renderHeader(parts, pane, info, window, floats, label) {
  const active = window.activePane === pane.id;
  parts.element.parentElement?.toggleAttribute("data-inactive", !active);
  parts.element.toggleAttribute("data-active", active);
  parts.title.querySelector(".title").textContent = label ?? info.label ?? "";
  parts.title.querySelector(".kind").dataset.icon = paneIcon(info.kind);
  parts.title.toggleAttribute("data-movable", floats ? writable() : canDrag(window));
  if (parts.floats !== floats) {
    parts.floats = floats;
    parts.actions.replaceChildren(
      ...headerActions(floats).map(([action, icon, title]) => iconButton("pane.header.button", action, icon, title)),
    );
  }
  const extras = headerButtons.map((factory) => factory(pane, info)).filter(Boolean);
  const current = [...parts.extras.children];
  if (extras.length !== current.length || extras.some((item, index) => item !== current[index])) {
    parts.extras.replaceChildren(...extras);
  }
}

function hairline(slot, node, axis) {
  slot.replaceChildren();
  if (metrics().gaps) return;
  const resizing = splitDrag?.split === node.split;
  slot.append(el("i", { class: `split-line${resizing ? " resizing" : ""}` }));
  if (!node.highlight || resizing) return;
  const segment = el("i", { class: `split-highlight ${node.highlight.side}` });
  const start = `${node.highlight.start * 100}%`;
  const length = `${node.highlight.length * 100}%`;
  if (axis === "h") Object.assign(segment.style, { top: start, height: length });
  else Object.assign(segment.style, { left: start, width: length });
  slot.append(segment);
}

function build(node, window, laid) {
  if (!node) return el("div", { class: "split-empty" });
  if ("pane" in node) {
    const info = window.panes.find((item) => item.id === node.pane);
    const pane = paneElement(node.pane);
    if (info) {
      mount(pane, info);
      chrome(pane, info, window, false);
    }
    laid.add(node.pane);
    return pane.element;
  }
  const ratio = splitDrag && splitDrag.split === node.split ? splitDrag.ratio : node.ratio;
  const { margin, gaps } = metrics();
  const slotSize = Math.max(gaps ? margin : 0, 1);
  const box = el("div", { class: `split ${node.axis}`, "data-split": node.split });
  const first = el("div", { class: "split-first" });
  const second = el("div", { class: "split-second" });
  const slot = el("div", { class: "split-slot" });
  first.style.flexBasis = `${ratio * 100}%`;
  slot.style.flexBasis = `${slotSize}px`;
  first.append(build(node.first, window, laid));
  second.append(build(node.second, window, laid));
  hairline(slot, node, node.axis);
  box.append(first, slot, second);
  if (writable() && !paneDrag) {
    const hit = el("div", { class: "split-hit" });
    hit.style.setProperty("--offset", `${-(16 - slotSize) / 2}px`);
    hit.style.setProperty("--ratio", ratio);
    hit.addEventListener("pointerdown", (event) => beginSplitDrag(event, window, node, box));
    box.append(hit);
  }
  return box;
}

function beginSplitDrag(event, window, node, box) {
  if (event.button !== 0) return;
  event.preventDefault();
  event.stopPropagation();
  try {
    event.target.setPointerCapture(event.pointerId);
  } catch {}
  splitDrag = { window: window.id, split: node.split, axis: node.axis, start: node.ratio, ratio: node.ratio, box, committed: null };
  box.querySelector(":scope > .split-slot .split-line")?.classList.add("resizing");
  box.querySelector(":scope > .split-slot .split-highlight")?.remove();
  const move = (moveEvent) => {
    if (splitDrag?.committed !== null) return;
    const rect = box.getBoundingClientRect();
    const [offset, size] = node.axis === "h" ? [moveEvent.clientX - rect.left, rect.width] : [moveEvent.clientY - rect.top, rect.height];
    splitDrag.ratio = size > 0 ? Math.min(Math.max(offset / size, 0), 1) : 0.5;
    box.querySelector(":scope > .split-first").style.flexBasis = `${splitDrag.ratio * 100}%`;
    box.querySelector(":scope > .split-hit")?.style.setProperty("--ratio", splitDrag.ratio);
  };
  const up = () => {
    event.target.removeEventListener("pointermove", move);
    event.target.removeEventListener("pointerup", up);
    event.target.removeEventListener("pointercancel", up);
    commitSplit();
  };
  event.target.addEventListener("pointermove", move);
  event.target.addEventListener("pointerup", up);
  event.target.addEventListener("pointercancel", up);
}

function commitSplit() {
  if (!splitDrag || splitDrag.committed !== null) return;
  const committed = state.client.split_commit(splitDrag.window, splitDrag.split, splitDrag.start, splitDrag.ratio);
  flush();
  if (committed < 0) {
    splitDrag = null;
    relayout();
    return;
  }
  splitDrag.ratio = committed;
  splitDrag.committed = state.tree?.generation ?? 0;
}

function relayout() {
  for (const pane of panes.values()) markPane(pane.id);
  markTree();
}

function paneRects(source) {
  const rects = [];
  for (const [id, pane] of panes) {
    if (id === source || !pane.element.isConnected || pane.floating) continue;
    rects.push([id, pane.element.getBoundingClientRect()]);
  }
  return rects;
}

function layoutKey(layout) {
  return JSON.stringify(layout, (key, value) => (key === "highlight" ? undefined : value));
}

function beginPaneDrag(id, event, requiresPrefix) {
  const window = activeWindow();
  if (!window || !canDrag(window) || !window.panes.some((pane) => pane.id === id)) return;
  const startX = event.clientX;
  const startY = event.clientY;
  const title = window.panes.find((pane) => pane.id === id)?.label || "";
  let started = false;
  const move = (moveEvent) => {
    if (!started) {
      if (Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) < 3) return;
      started = true;
      paneDrag = { pane: id, window: window.id, layout: layoutKey(window.layout), requiresPrefix, target: null, rendered: false };
      chip.querySelector("b").textContent = `%${id}`;
      chip.querySelector("span").textContent = title;
      chip.hidden = false;
      markTree();
    }
    chip.style.transform = `translate(${moveEvent.clientX + 12}px, ${moveEvent.clientY + 12}px)`;
    updateDropTarget(moveEvent.clientX, moveEvent.clientY);
  };
  const end = (upEvent) => {
    removeEventListener("pointermove", move);
    removeEventListener("pointerup", end);
    removeEventListener("pointercancel", end);
    if (!started) {
      if (requiresPrefix && upEvent.type === "pointerup") {
        focusPane(id);
        state.client.dismiss_prefix(id);
        flush();
      }
      return;
    }
    dropPane(upEvent.type === "pointerup" ? upEvent : null);
  };
  addEventListener("pointermove", move);
  addEventListener("pointerup", end);
  addEventListener("pointercancel", end);
}

function updateDropTarget(x, y) {
  if (!paneDrag) return;
  const hit = paneRects(paneDrag.pane).find(([, rect]) => x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom);
  let target = null;
  if (hit) {
    const [id, rect] = hit;
    const zone = state.client.pane_drop_zone(paneDrag.window, paneDrag.pane, id, new Float32Array([rect.left, rect.top, rect.width, rect.height]), x, y);
    if (zone) target = { pane: id, zone, rect };
  }
  const same = (paneDrag.target && target && paneDrag.target.pane === target.pane && paneDrag.target.zone === target.zone) || (!paneDrag.target && !target);
  if (same) return;
  paneDrag.target = target;
  const animate = preferences.animations && !matchMedia("(prefers-reduced-motion: reduce)").matches;
  preview.style.transitionDuration = animate && paneDrag.rendered ? (target ? "180ms" : "140ms") : "0ms";
  if (!target) {
    preview.style.opacity = "0";
    return;
  }
  const origin = canvas.getBoundingClientRect();
  const { margin, gaps } = metrics();
  const [px, py, pw, ph] = state.client.pane_drop_preview(
    new Float32Array([target.rect.left - origin.left, target.rect.top - origin.top, target.rect.width, target.rect.height]),
    target.zone,
    gaps ? Math.max(margin, 1) : 1,
  );
  if (!paneDrag.rendered) {
    preview.style.transitionDuration = "0ms";
    Object.assign(preview.style, { left: `${px}px`, top: `${py}px`, width: `${pw}px`, height: `${ph}px` });
    preview.getBoundingClientRect();
    preview.style.transitionDuration = animate ? "180ms" : "0ms";
  }
  paneDrag.rendered = true;
  Object.assign(preview.style, { left: `${px}px`, top: `${py}px`, width: `${pw}px`, height: `${ph}px`, opacity: "1" });
}

function dropPane(event) {
  const drag = paneDrag;
  if (!drag) return;
  if (event) updateDropTarget(event.clientX, event.clientY);
  paneDrag = null;
  chip.hidden = true;
  preview.style.opacity = "0";
  if (event && drag.target && writable()) {
    const predicted = state.client.pane_drop(drag.window, drag.pane, drag.target.pane, drag.target.zone);
    if (predicted && predicted !== "null") {
      override = { window: drag.window, layout: JSON.parse(predicted), generation: state.tree?.generation ?? 0 };
    }
  }
  if (drag.requiresPrefix) state.client.dismiss_prefix(drag.pane);
  flush();
  markTree();
}

function cancelDrags() {
  if (paneDrag) {
    const drag = paneDrag;
    paneDrag = null;
    chip.hidden = true;
    preview.style.opacity = "0";
    if (drag.requiresPrefix) {
      state.client.dismiss_prefix(drag.pane);
      flush();
    }
  }
}

function veils(window) {
  const draggable = canDrag(window) && (prefixArmed || paneDrag);
  for (const pane of panes.values()) {
    const veil = pane.veil;
    if (!draggable || !pane.element.isConnected) {
      veil.hidden = true;
      continue;
    }
    veil.hidden = false;
    veil.dataset.drag = !paneDrag ? "armed" : paneDrag.pane === pane.id ? "source" : "target";
    if (veil.dataset.drag === "armed") {
      veil.dataset.zz = "pane.drop-zone";
      veil.dataset.state = "hover";
    } else {
      delete veil.dataset.zz;
      delete veil.dataset.state;
    }
  }
}

function cellSize() {
  return [state.cell.width || 8, state.cell.height || 18];
}

function floatCanvas() {
  return [content.clientWidth, content.clientHeight];
}

function floatRows() {
  const window = activeWindow();
  if (!window) return 0;
  return state.client.float_rows(window.id, new Float32Array(cellSize()), new Float32Array(floatCanvas()), metrics().header);
}

function floatSurface(pane, float, info, window) {
  const active = window.activePane === pane.id;
  let surface = pane.surface;
  if (!surface) {
    surface = el("div", { class: "float-surface" });
    surface.addEventListener("pointerdown", (event) => event.stopPropagation());
    pane.surface = surface;
  }
  surface.dataset.zz = float.modal ? "pane.float" : "pane.pip";
  surface.toggleAttribute("data-bordered", float.bordered);
  surface.toggleAttribute("data-modal", float.modal);
  surface.dataset.state = float.modal && active ? "current" : "";
  const [insetX, insetY] = float.inset;
  const { header: headerHeight } = metrics();
  const docked = !float.modal && float.bordered && insetY > 0;
  const top = float.modal ? insetY : docked ? Math.max(headerHeight, insetY) : insetY;
  surface.style.setProperty("--inset-x", `${insetX}px`);
  surface.style.setProperty("--inset-top", `${top}px`);
  surface.style.setProperty("--inset-y", `${insetY}px`);
  surface.style.setProperty("--header", `${Math.max(headerHeight, insetY)}px`);
  if (float.modal) {
    let label = surface.querySelector(":scope > .float-title");
    if (!label) {
      label = el("div", { class: "float-title zz-mono" }, `<span data-text></span>`);
      surface.append(label);
    }
    label.hidden = !float.bordered || !float.title;
    label.firstChild.textContent = float.title;
    surface.querySelector(":scope > .pip-header")?.remove();
  } else {
    surface.querySelector(":scope > .float-title")?.remove();
    let holder = surface.querySelector(":scope > .pip-header");
    if (!holder) {
      holder = el("div", { class: "pip-header" });
      surface.append(holder);
    }
    holder.toggleAttribute("data-docked", docked);
    if (!pane.pipHeader) {
      pane.pipHeader = header(pane);
    }
    renderHeader(pane.pipHeader, pane, info, window, true, float.title || info.label);
    if (holder.firstChild !== pane.pipHeader.element) holder.replaceChildren(pane.pipHeader.element);
    holder.toggleAttribute("data-inactive", !active);
  }
  if (pane.element.parentElement !== surface) surface.prepend(pane.element);
  return surface;
}

function renderFloats(window) {
  const list = JSON.parse(state.client.floats(window.id, new Float32Array(cellSize()), new Float32Array(floatCanvas()), metrics().header));
  const layer = [];
  for (const float of list) {
    const info = window.panes.find((item) => item.id === float.pane);
    if (!info) continue;
    const pane = paneElement(float.pane);
    mount(pane, info);
    chrome(pane, info, window, true);
    if (float.modal) {
      const scrim = el("div", { class: "modal-scrim" });
      scrim.addEventListener("pointerdown", (event) => {
        event.stopPropagation();
        if (float.closeOnClick && writable()) {
          state.client.pane_action(float.pane, "close", 0);
          flush();
        }
      });
      layer.push(scrim);
    }
    const holder = el("div", { class: "float-frame" });
    let [x, y, width, height] = float.frame;
    if (pipDrag?.pane === float.pane) [x, y] = [pipDrag.x, pipDrag.y];
    Object.assign(holder.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` });
    holder.dataset.pane = float.pane;
    pane.frameRect = [x, y, width, height];
    holder.append(floatSurface(pane, float, info, window));
    layer.push(holder);
  }
  floatLayer.replaceChildren(...layer);
  floatLayer.hidden = !layer.length;
  return new Set(list.map((float) => float.pane));
}

function beginPip(pane, event) {
  const window = activeWindow();
  if (!window || !writable() || !pane.frameRect) return;
  event.preventDefault();
  const [x, y] = pane.frameRect;
  const start = { x: event.clientX, y: event.clientY };
  let moved = false;
  focusPane(pane.id);
  const move = (moveEvent) => {
    const dx = moveEvent.clientX - start.x;
    const dy = moveEvent.clientY - start.y;
    if (!moved && Math.hypot(dx, dy) < 3) return;
    moved = true;
    pipDrag = { pane: pane.id, x: x + dx, y: y + dy };
    const holder = floatLayer.querySelector(`.float-frame[data-pane="${pane.id}"]`);
    if (holder) Object.assign(holder.style, { left: `${pipDrag.x}px`, top: `${pipDrag.y}px` });
  };
  const end = () => {
    removeEventListener("pointermove", move);
    removeEventListener("pointerup", end);
    removeEventListener("pointercancel", end);
    if (!moved || !pipDrag) return;
    const [, , width, height] = pane.frameRect;
    state.client.float_land(window.id, pane.id, new Float32Array(cellSize()), new Float32Array(floatCanvas()), metrics().header, new Float32Array([pipDrag.x + width / 2, pipDrag.y + height / 2]));
    flush();
    pipDrag.landing = state.tree?.generation ?? 0;
    const landing = pipDrag;
    setTimeout(() => {
      if (pipDrag !== landing) return;
      pipDrag = null;
      markTree();
    }, 1000);
  };
  addEventListener("pointermove", move);
  addEventListener("pointerup", end);
  addEventListener("pointercancel", end);
}

function windowTab(entry) {
  const tab = el("div", { class: "window-tab", "data-zz": "titlebar.window-tab", "data-window": entry.id, title: entry.label });
  if (entry.active) tab.dataset.state = "current";
  const deck = el("div", { class: "tab-deck" });
  entry.panes.slice(0, 3).forEach((pane, index) => {
    const tile = el("span", { class: "deck-tile", "data-zz": "pane-stack" }, `<i data-icon="${paneIcon(pane.tag)}"></i>`);
    if (index === 0) tile.dataset.state = "current";
    tile.style.left = `${index * 5}px`;
    tile.style.zIndex = 3 - index;
    deck.append(tile);
  });
  deck.style.width = `${20 + (Math.min(entry.panes.length, 3) - 1) * 5}px`;
  const name = el("span", { class: "tab-name", "aria-label": entry.label });
  name.textContent = entry.label;
  const dots = el("span", { class: "tab-dots" }, `<i class="dot bell"></i><i class="dot activity"></i>`);
  dots.querySelector(".bell").style.opacity = entry.bell ? 1 : 0;
  dots.querySelector(".activity").style.opacity = entry.activity ? 1 : 0;
  const close = el("button", { "data-zz": "sidebar.row-action", class: "tab-close", title: "Close window" }, `<i data-icon="xmark"></i>`);
  close.disabled = !writable();
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    state.client.window_command(entry.id, "close");
    flush();
  });
  tab.append(entry.panes.length ? deck : "", name, dots, close);
  tab.addEventListener("click", () => {
    if (!state.connected) return;
    state.client.window_command(entry.id, "select");
    flush();
  });
  tab.addEventListener("contextmenu", (event) => {
    if (!writable()) return;
    event.preventDefault();
    contextMenu(event.clientX, event.clientY, [
      [entry.rename, () => state.client.window_command(entry.id, "rename")],
      ["Close Window", () => state.client.window_command(entry.id, "close"), "xmark"],
    ]);
  });
  return tab;
}

export function contextMenu(x, y, items) {
  document.querySelector(".zz-context-menu")?.remove();
  const menu = el("div", { class: "zz-menu zz-context-menu", "data-zz": "popover" });
  menu.setAttribute("role", "menu");
  for (const [label, run, icon] of items) {
    const item = el("div", { "data-zz": "menu-item" });
    item.setAttribute("role", "menuitem");
    item.innerHTML = `${icon ? `<i data-icon="${icon}"></i>` : ""}<span data-text></span>`;
    item.querySelector("span").textContent = label;
    item.addEventListener("click", () => {
      menu.remove();
      run();
      flush();
    });
    item.addEventListener("mouseenter", () => (item.dataset.state = "selected"));
    item.addEventListener("mouseleave", () => delete item.dataset.state);
    menu.append(item);
  }
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  document.body.append(menu);
  const bounds = menu.getBoundingClientRect();
  if (bounds.right > innerWidth - 8) menu.style.left = `${Math.max(8, innerWidth - 8 - bounds.width)}px`;
  if (bounds.bottom > innerHeight - 8) menu.style.top = `${Math.max(8, innerHeight - 8 - bounds.height)}px`;
  const dismiss = (event) => {
    if (menu.contains(event.target)) return;
    menu.remove();
    removeEventListener("pointerdown", dismiss, true);
  };
  setTimeout(() => addEventListener("pointerdown", dismiss, true));
}

export function narrow() {
  return innerWidth < 640;
}

export function sidebarInline() {
  return !narrow() && preferences.sidebar;
}

function renderTitlebar() {
  const inline = sidebarInline();
  titlebar.hidden = inline;
  if (inline || !state.client) return;
  const model = JSON.parse(state.client.status_bar());
  const strip = el("div", { class: "window-strip" });
  strip.append(...model.windows.map(windowTab));
  if (model.overflow) {
    const more = el("button", { "data-zz": "chrome.button", class: "overflow", title: "All windows" }, `<i data-icon="ellipsis"></i>`);
    more.disabled = !state.connected;
    more.addEventListener("click", (event) => {
      const rect = more.getBoundingClientRect();
      event.stopPropagation();
      contextMenu(
        rect.left,
        rect.bottom + 4,
        model.overflow.map((entry) => [entry.label, () => state.client.window_command(entry.id, "select"), entry.active ? "check" : "app-window"]),
      );
    });
    strip.append(more);
  }
  const toggle = el("button", { "data-zz": "chrome.button", class: "titlebar-toggle", title: "Toggle sidebar" }, `<i data-icon="panel-left"></i>`);
  toggle.addEventListener("click", () => emit("toggleSidebar"));
  const controls = el("div", { class: "titlebar-controls" });
  controls.append(toggle);
  const body = el("div", { class: "titlebar-content" });
  body.append(strip);
  titlebar.replaceChildren(el("div", { class: "titlebar-leading" }), controls, body);
}

function applyMetrics() {
  const { margin, header, titlebar: height, gaps } = metrics();
  const root = app.style;
  root.setProperty("--zz-pane-header-height", `${header}px`);
  root.setProperty("--zz-titlebar-height", `${height}px`);
  root.setProperty("--pane-margin", `${margin}px`);
  const glow = preferences.pane_glow_strength;
  root.setProperty("--pane-wash-scale", glow);
  root.setProperty("--pip-opacity", preferences.pip_background_opacity);
  root.setProperty("--pane-rim", Math.max(0, 1 - 0.3 * glow));
  app.classList.toggle("zz-gaps", gaps);
  app.toggleAttribute("data-gaps", gaps);
  app.toggleAttribute("data-animations", preferences.animations);
}

function arrivals(window) {
  const ids = new Set(window.panes.map((pane) => pane.id));
  if (arrived.window === window.id && preferences.animations) {
    for (const id of ids) {
      if (!arrived.known.has(id)) panes.get(id)?.element.classList.add("arriving");
    }
  }
  arrived = { window: window.id, known: ids };
}

function renderEmpty() {
  const tree = state.tree;
  empty.hidden = false;
  empty.querySelector(".empty-status").textContent = state.connected ? "Connected" : $("#status")?.textContent || "";
  const button = empty.querySelector("button");
  const hasSessions = Boolean(tree?.sessions.length);
  button.textContent = hasSessions ? "Attach to session" : "Create session";
  button.onclick = () => {
    const first = state.tree?.sessions[0];
    if (first) state.client.attach(`$${first.id}`);
    else state.client.sidebar_command("host", "add");
    flush();
  };
}

function render() {
  applyMetrics();
  renderTitlebar();
  const window = activeWindow();
  const tree = state.tree;
  if (override && (!window || override.window !== window.id || window.zoomedPane || (tree?.generation ?? 0) > override.generation)) override = null;
  if (splitDrag && (!window || splitDrag.window !== window.id || window.zoomedPane)) splitDrag = null;
  if (splitDrag?.committed !== null && splitDrag && (tree?.generation ?? 0) > splitDrag.committed) {
    splitDrag = null;
    for (const pane of panes.values()) markPane(pane.id);
  }
  prefixArmed = Boolean(tree?.prefix);
  if (pipDrag?.landing !== undefined && (tree?.generation ?? 0) > pipDrag.landing) pipDrag = null;
  if (paneDrag && (!window || paneDrag.window !== window.id || layoutKey(window.layout) !== paneDrag.layout || window.zoomedPane || !writable() || (paneDrag.requiresPrefix && !prefixArmed))) cancelDrags();
  if (!window) {
    content.replaceChildren();
    renderEmpty();
    return;
  }
  empty.hidden = true;
  arrivals(window);
  const root = window.zoomedPane ? { pane: window.zoomedPane } : override?.layout || window.layout;
  const laid = new Set();
  const element = build(root, window, laid);
  element.classList.add("layout-root");
  const floated = renderFloats(window);
  if (content.firstChild !== element || content.childElementCount !== 2) content.replaceChildren(element, floatLayer);
  for (const id of floated) laid.add(id);
  const alive = new Set(session()?.windows.flatMap((other) => other.panes.map((pane) => pane.id)) || []);
  for (const [id, pane] of panes) {
    if (!alive.has(id)) {
      pane.view?.destroy?.();
      pane.element.remove();
      panes.delete(id);
      pickers.delete(id);
    } else if (!laid.has(id) && pane.element.isConnected) {
      pane.element.remove();
    }
  }
  veils(window);
  for (const pane of window.panes) markPane(pane.id);
}

phase("tree", render);
phase("panes", (ids) => {
  for (const id of ids) panes.get(id)?.view?.update?.();
});
function fades() {
  for (const label of document.querySelectorAll(".pane-title > .title, .tab-name")) {
    label.toggleAttribute("data-fade", label.scrollWidth > label.clientWidth + 0.5);
  }
}

phase("after", () => {
  fades();
  if (!splitDrag) for (const pane of panes.values()) if (pane.element.isConnected) pane.view?.layout?.();
  const tree = state.tree;
  toast.hidden = state.connected || !tree;
  if (!toast.hidden) toast.textContent = $("#status")?.textContent || "";
  flush();
});

on("prefix", ({ armed }) => {
  prefixArmed = armed;
  if (paneDrag?.requiresPrefix && !armed) cancelDrags();
  const window = activeWindow();
  if (window) veils(window);
});

on("commandResponse", (event) => {
  if (event.response && typeof event.response === "object" && "Error" in event.response) {
    override = null;
    splitDrag = null;
    markTree();
  }
});

on("attached", () => {
  override = null;
  splitDrag = null;
  cancelDrags();
});

on("preferences", () => {
  Object.assign(preferences, DEFAULTS, stored());
  markTree();
});


addEventListener("keydown", (event) => {
  if (event.key === "Escape" && paneDrag) {
    event.preventDefault();
    cancelDrags();
    markTree();
  }
});
addEventListener("blur", () => {
  if (paneDrag) {
    cancelDrags();
    markTree();
  }
});

content.addEventListener("animationend", (event) => event.target.classList.remove("arriving"));

new ResizeObserver(() => {
  for (const pane of panes.values()) markPane(pane.id);
  const window = activeWindow();
  if (window?.panes.some((pane) => panes.get(pane.id)?.floating)) markTree();
  schedule();
}).observe(canvas);

on("focus", () => {
  for (const pane of panes.values()) markPane(pane.id);
});

function picker(pane) {
  const choices = JSON.parse(state.client.picker_choices());
  pane.body.innerHTML = `<div class="pane-picker"><div class="picker-choices"></div></div>`;
  const list = pane.body.querySelector(".picker-choices");
  const view = {
    selected: 0,
    update() {
      list.replaceChildren(
        ...choices.map((choice, index) => {
          const row = el("div", { class: "picker-row", "data-zz": "pane.picker" });
          if (index === view.selected) row.dataset.state = "current";
          if (!writable()) row.dataset.state = `${row.dataset.state || ""} disabled`.trim();
          row.innerHTML = `<i data-icon="${paneIcon(choice.kind)}"></i><span class="picker-title"></span><span data-zz="keycap"><span data-text></span></span>`;
          row.querySelector(".picker-title").textContent = choice.title;
          row.querySelector(".picker-title").setAttribute("aria-label", choice.title);
          row.querySelector("[data-zz='keycap'] > span").textContent = choice.shortcut;
          row.addEventListener("mousemove", () => {
            if (view.selected !== index) {
              view.selected = index;
              view.update();
            }
          });
          row.addEventListener("pointerdown", (event) => {
            event.stopPropagation();
            if (!writable()) return;
            state.client.picker_choose(pane.id, index);
            flush();
          });
          return row;
        }),
      );
    },
    layout() {},
    destroy() {
      pickers.delete(pane.id);
    },
  };
  pane.body.addEventListener("pointerdown", () => {
    state.client.focus_pane(pane.id);
    flush();
  });
  pickers.set(pane.id, view);
  view.update();
  return view;
}

paneRenderer("picker", picker);

onKey((event) => {
  const window = activeWindow();
  if (!window || event.isComposing || event.target.closest?.("input, textarea, select, [contenteditable]")) return false;
  const view = pickers.get(window.activePane);
  if (!view) return false;
  const result = state.client.picker_key(window.activePane, view.selected, event.key, event.shiftKey, event.ctrlKey, event.altKey, event.metaKey);
  if (result === -2) return false;
  event.preventDefault();
  if (result >= 0) {
    view.selected = result;
    view.update();
  }
  flush();
  return true;
});

function unsupported(title, icon, reason) {
  return (pane) => {
    pane.body.innerHTML = `<div class="unsupported-pane"><i data-icon="${icon}"></i><div data-text>${title}</div><div class="zz-muted reason" data-text>${reason}</div></div>`;
    return { update() {}, layout() {}, destroy() {} };
  };
}

paneRenderer("editor", unsupported("Editor", "file", "The daemon does not share editor file contents with browser clients yet."));
