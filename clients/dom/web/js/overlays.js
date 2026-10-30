import { $, flush, on, onKey, phase, schedule, state } from "./core.js";

const root = $("#overlays");
const layers = {};
for (const name of ["floating", "modal", "toasts"]) {
  layers[name] = document.createElement("div");
  layers[name].className = `zz-layer-${name}`;
  root.append(layers[name]);
}

const THEME_COLOURS = {
  0: "var(--penna-bg)",
  1: "var(--penna-fg)",
  2: "var(--penna-border)",
  3: "var(--penna-panel)",
  4: "var(--penna-success)",
  5: "var(--penna-warning)",
  6: "var(--penna-danger)",
  7: "var(--penna-fg)",
  8: "var(--penna-fg)",
  9: "var(--penna-fg)",
};

export function colour(value) {
  if (!value) return null;
  if (value.startsWith("theme:")) return THEME_COLOURS[value.slice(6)] || null;
  return value;
}

export function element(tag, attributes = {}, ...children) {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (value === null || value === undefined || value === false) continue;
    if (name === "class") node.className = value;
    else if (name === "text") node.textContent = value;
    else if (name.startsWith("on")) node.addEventListener(name.slice(2), value);
    else node.setAttribute(name, value === true ? "" : value);
  }
  node.append(...children.flat(Infinity).filter((child) => child !== null && child !== undefined && child !== false));
  return node;
}

export function icon(name) {
  return element("i", { "data-icon": name });
}

export function segments(list, base = {}) {
  return list.map((segment) => {
    const span = element("span", { text: segment.text });
    let fg = colour(segment.fg);
    let bg = colour(segment.bg);
    if (segment.reverse) [fg, bg] = [bg || base.bg || "var(--penna-fill)", fg || base.fg || "currentColor"];
    if (fg) span.style.color = fg;
    if (bg) span.style.background = bg;
    if (segment.bold !== null) span.style.fontWeight = segment.bold ? "700" : "400";
    if (segment.italic !== null) span.style.fontStyle = segment.italic ? "italic" : "normal";
    const lines = [segment.underline && "underline", segment.strike && "line-through"].filter(Boolean);
    if (lines.length) span.style.textDecoration = lines.join(" ");
    if (segment.underline && segment.us) span.style.textDecorationColor = colour(segment.us);
    if (segment.fade) span.style.opacity = String(1 - segment.fade);
    return span;
  });
}

export function plain(list) {
  return list.map((segment) => segment.text).join("");
}

export function glyphs(parts) {
  return parts.map((part) => (part.icon ? icon(part.icon) : element("span", { "data-text": true, text: part.text })));
}

function cell(className, text) {
  return element("span", { class: className, "aria-label": text || null, text });
}

export function keyArgs(event) {
  const text = event.key.length === 1 ? event.key : null;
  let key = event.key;
  if (event.code?.startsWith("Key")) key = event.code.slice(3).toLowerCase();
  else if (event.code?.startsWith("Digit")) key = event.code.slice(5);
  return [key, text, event.shiftKey, event.ctrlKey, event.altKey, event.metaKey];
}

function workspaceRect() {
  return $("#workspace").getBoundingClientRect();
}

function clampInto(node, left, top) {
  const width = node.offsetWidth;
  const height = node.offsetHeight;
  const margin = 8;
  const x = Math.max(margin, Math.min(left, innerWidth - margin - width));
  const y = Math.max(margin, Math.min(top, innerHeight - margin - height));
  node.style.left = `${x}px`;
  node.style.top = `${y}px`;
}

let menuNode = null;

function menuPointer(kind, buttons, event) {
  if (!menuNode) return;
  const surface = menuNode.getBoundingClientRect();
  const rows = menuNode.querySelector(".zz-menu-rows").getBoundingClientRect();
  const tops = [...menuNode.querySelector(".zz-menu-rows").children].map((child) => child.getBoundingClientRect().top);
  const changed = state.client.menu_pointer(
    kind,
    buttons,
    event.clientX,
    event.clientY,
    [surface.x, surface.y, surface.width, surface.height],
    [rows.x, rows.y, rows.width, rows.height],
    tops,
  );
  flush();
  if (changed) renderFloating();
}

function renderMenu(menu, area) {
  const surface = element("div", {
    "data-penna": "popover",
    class: "zz-display-menu",
    role: "menu",
    "data-titled": menu.titled,
    "data-outlined": menu.outlined,
  });
  if (menu.fill) surface.style.setProperty("--penna-fill", menu.fill);
  if (menu.text) surface.style.color = menu.text;
  if (menu.outline && menu.outlined) {
    surface.style.setProperty("--penna-edge", menu.outline);
    surface.style.setProperty("--penna-bw", "max(var(--penna-outline), 1px)");
  }
  if (!menu.outlined) surface.style.setProperty("--penna-bw", "0px");
  const names = [];
  if (menu.titled) {
    const title = element(
      "div",
      { class: "zz-menu-title" },
      ...menu.title.map((bucket, index) => {
        const text = plain(bucket);
        if (text) names.push(text);
        return element("span", { class: ["start", "centre", "end"][index], "data-text": text ? true : null }, segments(bucket));
      }),
    );
    surface.append(title);
  }
  const rows = element("div", { class: "zz-menu-rows" });
  rows.style.setProperty("--row-height", `${menu.rowHeight}px`);
  menu.items.forEach((item) => {
    if (!item) {
      rows.append(element("div", { class: "zz-menu-rule" }));
      return;
    }
    const name = plain(item.name);
    const label = item.annotation ? `${name} ${item.annotation}` : name;
    names.push(label);
    const row = element(
      "div",
      {
        "data-penna": "menu-item",
        role: "menuitem",
        "aria-label": label,
        "aria-selected": item.selected ? "true" : null,
        "data-enabled": item.enabled,
      },
      element("span", { class: "zz-menu-name", "data-text": true }, segments(item.name)),
      item.annotation && element("span", { class: "zz-menu-key", "data-text": true, text: item.annotation }),
    );
    if (item.selected && menu.selectedFill) row.style.setProperty("--penna-fill", menu.selectedFill);
    if (item.selected && menu.selectedText) row.style.color = menu.selectedText;
    rows.append(row);
  });
  surface.append(rows);
  surface.setAttribute("aria-label", names.join(" "));
  const shield = element("div", { class: "zz-menu-shield" });
  shield.style.left = `${area.x}px`;
  shield.style.top = `${area.y}px`;
  shield.style.width = `${area.width}px`;
  shield.style.height = `${area.height}px`;
  shield.addEventListener("mousedown", (event) => {
    event.preventDefault();
    menuPointer("press", event.button, event);
  });
  shield.addEventListener("mouseup", (event) => menuPointer("release", 3, event));
  shield.addEventListener("mousemove", (event) => menuPointer(event.buttons ? "drag" : "motion", event.buttons ? [0, 2, 1][Math.log2(event.buttons & 7) | 0] : 3, event));
  shield.addEventListener("wheel", (event) => {
    event.preventDefault();
    menuPointer("wheel", 64, event);
  }, { passive: false });
  shield.addEventListener("contextmenu", (event) => event.preventDefault());
  return [shield, surface];
}

function renderConfirm(confirm, area) {
  const node = element(
    "div",
    { "data-penna": "pane.float", class: "zz-confirm" },
    element("div", { class: "zz-confirm-prompt" }, element("span", { "data-text": true, text: confirm.prompt })),
  );
  node.style.width = `${confirm.width}px`;
  node.style.height = `${confirm.height}px`;
  node.style.left = `${area.x + Math.max(0, (area.width - confirm.width) / 2)}px`;
  node.style.top = `${area.y + Math.max(0, (area.height - confirm.height) / 2)}px`;
  return node;
}

function renderFloating() {
  const client = state.client;
  layers.floating.replaceChildren();
  menuNode = null;
  if (!client) return;
  const area = workspaceRect();
  const menu = client.menu(area.width, area.height, devicePixelRatio || 1, innerHeight);
  if (menu) {
    const parsed = JSON.parse(menu);
    const [shield, surface] = renderMenu(parsed, area);
    layers.floating.append(shield, surface);
    clampInto(surface, area.x + parsed.frame.x, area.y + parsed.frame.y);
    menuNode = surface;
    return;
  }
  const confirm = client.confirm(area.width);
  if (confirm) layers.floating.append(renderConfirm(JSON.parse(confirm), area));
}

function chooserRow(chooser, row, index) {
  const node = element("div", {
    "data-penna": "menu-item",
    class: `zz-chooser-row ${chooser.kind}`,
    "aria-selected": row.selected ? "true" : null,
  });
  const parts = [];
  if (chooser.keys) parts.push(cell("zz-chooser-key penna-mono", row.key));
  if (chooser.kind === "tree") {
    const depth = element("span", { class: "zz-chooser-depth" });
    depth.style.width = `${row.depth * 16}px`;
    parts.push(
      depth,
      element("span", { class: "zz-chooser-disclosure" }, row.disclosure === null ? null : icon(row.disclosure ? "chevron-down" : "chevron-right")),
      element("span", { class: "zz-chooser-icon", "data-active": row.active }, icon(row.icon)),
      element("span", { class: "zz-chooser-label", "aria-label": row.label || null, "data-active": row.active, text: row.label }),
      element("span", { class: "zz-chooser-detail", "data-text": row.detail ? true : null, text: row.detail }),
      element("span", { class: "zz-chooser-target penna-mono", "data-text": true, text: row.target }),
      row.tagged && element("span", { "data-penna": "badge", "data-size": "small", class: "zz-chooser-tag", "data-text": true, text: "Tagged" }),
      element("span", { class: "zz-chooser-check" }, row.active ? icon("check") : null),
    );
  } else {
    parts.push(
      cell("zz-chooser-name penna-mono", row.name),
      cell("zz-chooser-preview", row.preview),
      cell("zz-chooser-size penna-mono", row.size),
      cell("zz-chooser-age penna-mono", row.age),
      row.tagged && element("span", { "data-penna": "badge", "data-size": "small", "data-tone": "primary", "data-prominent": true, class: "zz-chooser-tag penna-mono", "data-text": true, text: "TAGGED" }),
    );
  }
  node.append(...parts.filter(Boolean));
  node.addEventListener("mousedown", (event) => {
    event.stopPropagation();
    state.client.chooser_action(event.detail >= 2 ? "activate" : "select", index);
    flush();
  });
  return node;
}

let chooserSelection = null;

function renderModal() {
  const client = state.client;
  if (!client) {
    layers.modal.replaceChildren();
    return;
  }
  const json = client.choose_tree() || client.choose_buffer(Date.now() / 1000);
  if (!json) {
    chooserSelection = null;
    layers.modal.replaceChildren();
    return;
  }
  const chooser = JSON.parse(json);
  const rows = Math.min(chooser.rows.length, 10);
  const notices = (chooser.help ? 1 : 0) + (chooser.prompt ? 1 : 0);
  const scroller = element("div", { class: "zz-chooser-rows" }, chooser.rows.map((row, index) => chooserRow(chooser, row, index)));
  const notice = (text) => element("div", { "data-penna": "bar", class: "zz-chooser-notice penna-mono", "data-text": true, text });
  const close = element("button", { "data-penna": "icon-button.flat", "data-size": "small", title: "Close" }, icon("xmark"));
  close.addEventListener("mousedown", (event) => event.stopPropagation());
  close.addEventListener("click", () => {
    state.client.chooser_action("close", 0);
    flush();
  });
  const footer = element(
    "div",
    { class: "zz-chooser-footer" },
    chooser.search &&
      element(
        "div",
        { class: "zz-chooser-search penna-mono" },
        element("span", { "data-text": true, text: chooser.search.prefix }),
        element("span", { class: "value", "data-text": chooser.search.value ? true : null, text: chooser.search.value }),
      ),
    element("span", { class: "grow" }),
    element(
      "div",
      { class: "zz-chooser-hints" },
      chooser.hints.map((hint) =>
        element(
          "span",
          { class: "zz-hint" },
          hint.keys.map((parts) => element("span", { "data-penna": "keycap" }, glyphs(parts))),
          element("span", { "data-text": true, text: hint.label }),
        ),
      ),
    ),
  );
  const modal = element(
    "div",
    { "data-penna": "picker", class: "zz-chooser", role: "dialog" },
    element(
      "div",
      { class: "zz-chooser-header" },
      element(
        "div",
        { class: "zz-chooser-heading" },
        element("div", { class: "zz-chooser-title", "data-text": true, text: chooser.title }),
        element("div", { class: "zz-chooser-subtitle", "aria-label": chooser.subtitle, text: chooser.subtitle }),
      ),
      element("span", { class: "grow" }),
      close,
    ),
    chooser.help && notice("Help: the next key closes this and does nothing else"),
    chooser.prompt && notice(chooser.prompt),
    scroller,
    footer,
  );
  modal.style.maxWidth = `${chooser.maxWidth}px`;
  modal.style.height = `${106 + rows * 40 + notices * 36}px`;
  modal.addEventListener("mousedown", (event) => event.stopPropagation());
  const scrim = element("div", { class: "zz-chooser-scrim" }, modal);
  scrim.addEventListener("mousedown", () => {
    state.client.chooser_action("close", 0);
    flush();
  });
  layers.modal.replaceChildren(scrim);
  const key = `${chooser.kind}:${chooser.selected}`;
  const selected = scroller.children[chooser.selected];
  if (selected && chooserSelection !== key) {
    const top = selected.offsetTop - scroller.offsetTop - (scroller.clientHeight - selected.offsetHeight) / 2;
    scroller.scrollTop = Math.max(0, top);
  }
  chooserSelection = key;
}

function renderPanes() {
  for (const stale of document.querySelectorAll(".zz-indicator-holder")) stale.remove();
  const json = state.client?.display_panes();
  if (!json) return;
  for (const indicator of JSON.parse(json).indicators) {
    const pane = document.querySelector(`[data-penna="pane"][data-pane="${indicator.pane}"]`);
    if (!pane) continue;
    const option = colour(indicator.colour);
    const ring = indicator.active ? option || "var(--penna-accent)" : null;
    const glyph = element("span", { class: "zz-indicator-glyph", "data-text": true, text: indicator.glyph });
    glyph.style.color = ring || option || (indicator.keyed ? "var(--penna-ink)" : "var(--penna-muted)");
    const caption = [];
    if (indicator.index !== null) caption.push(element("span", { "data-text": true, text: indicator.index }));
    for (const bucket of indicator.label) {
      if (bucket.length) caption.push(element("span", { "data-text": true }, segments(bucket)));
    }
    const chip = element(
      "div",
      {
        "data-penna": "pane.display-panes",
        class: "zz-indicator",
        "data-state": indicator.active ? "current" : null,
        "data-captioned": caption.length > 0,
      },
      glyph,
      caption.length && element("span", { class: "zz-indicator-caption" }, caption),
    );
    chip.addEventListener("mousedown", (event) => event.stopPropagation());
    chip.addEventListener("mouseup", (event) => {
      event.stopPropagation();
      state.client.display_panes_select(indicator.pane);
      flush();
    });
    pane.append(element("div", { class: "zz-indicator-holder" }, chip));
  }
}

const TOAST_ICONS = { info: "info", success: "circle-check", warning: "triangle-alert", error: "circle-x" };
const toasts = new Map();
let toastCount = 0;

function dismissToast(id) {
  const toast = toasts.get(id);
  if (!toast) return;
  clearTimeout(toast.timer);
  toasts.delete(id);
  toast.node.dataset.closing = "";
  setTimeout(() => toast.node.remove(), 150);
}

export function toast(kind, text, durationMs = 5000, key = null) {
  const id = key ?? `toast-${++toastCount}`;
  if (toasts.has(id)) {
    clearTimeout(toasts.get(id).timer);
    toasts.get(id).node.remove();
    toasts.delete(id);
  }
  const close = element("button", { "data-penna": "icon-button.flat", "data-size": "small", class: "zz-toast-close" }, icon("xmark"));
  close.addEventListener("click", () => dismissToast(id));
  const node = element(
    "div",
    { "data-penna": "toast", class: "zz-toast", "data-kind": kind },
    element(
      "div",
      { class: "zz-toast-body" },
      TOAST_ICONS[kind] && element("span", { class: "zz-toast-icon" }, icon(TOAST_ICONS[kind])),
      element("div", { class: "zz-toast-message", "aria-label": text, text }),
    ),
    close,
  );
  node.addEventListener("auxclick", (event) => {
    if (event.button === 1) dismissToast(id);
  });
  layers.toasts.append(node);
  while (layers.toasts.children.length > 10) layers.toasts.firstChild.remove();
  const timer = durationMs === 0 ? null : setTimeout(() => dismissToast(id), durationMs);
  toasts.set(id, { node, timer });
  return id;
}

function render() {
  renderFloating();
  renderModal();
  renderPanes();
}

let pending = false;
export function refresh() {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    render();
  });
}

on("menu", () => {
  state.client.menu_changed();
  refresh();
});
for (const type of ["confirm", "chooseTree", "chooseBuffer", "displayPanes", "commandPrompt", "attached", "disconnected", "tree"]) on(type, refresh);
phase("after", renderPanes);
addEventListener("resize", refresh);

on("clientMessage", (event) => {
  const kind = typeof event.kind === "string" ? event.kind.toLowerCase() : "info";
  const duration = event.durationMs === null || event.durationMs === undefined ? 5000 : event.durationMs;
  toast(kind, event.text, duration, event.messageId === null || event.messageId === undefined ? null : `message-${event.messageId}`);
});

on("clientMessageCleared", (event) => dismissToast(`message-${event.messageId}`));

on("focus", ({ focused }) => {
  if (!focused && state.client) {
    state.client.cancel_prefix();
    flush();
  }
});
on("prefixCancelled", (event) => state.client.prefix_cancelled(event.requestId));
on("attached", () => state.client.reset_prefix_cancel());
on("disconnected", () => state.client?.reset_prefix_cancel());

onKey((event) => {
  const client = state.client;
  if (!client || event.isComposing) return false;
  if (client.prefix_cancel_pending() && !event.metaKey) {
    event.preventDefault();
    return true;
  }
  if (event.target.closest?.("#overlays input, #overlays textarea")) return false;
  if (["Shift", "Control", "Alt", "Meta", "CapsLock"].includes(event.key)) return false;
  if (!client.overlay_key(...keyArgs(event))) return false;
  event.preventDefault();
  flush();
  refresh();
  return true;
});

document.addEventListener("keyup", (event) => {
  const client = state.client;
  if (!client) return;
  const [key, , shift, control, alt, meta] = keyArgs(event);
  if (client.overlay_key_up(key, shift, control, alt, meta)) flush();
});

document.addEventListener(
  "paste",
  (event) => {
    const client = state.client;
    const text = event.clipboardData?.getData("text/plain");
    if (!client || !text) return;
    const remainder = client.menu_paste(text);
    if (remainder === undefined || remainder === null) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const pane = state.tree && document.querySelector("[data-pane][data-active]")?.dataset.pane;
    if (remainder && pane) client.paste(Number(pane), remainder);
    flush();
    refresh();
  },
  true,
);

schedule();
