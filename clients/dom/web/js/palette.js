import { $, flush, on, onKey, state } from "./core.js";
import { focusTerminal } from "./input.js";
import { element, glyphs, icon, keyArgs, toast } from "./overlays.js";

const layer = document.createElement("div");
layer.className = "zz-layer-palette";
$("#overlays").append(layer);

const view = { node: null, input: null, rows: null, last: null, scrolled: null };
const pointer = { x: null, y: null };

addEventListener("mousemove", (event) => {
  pointer.x = event.clientX;
  pointer.y = event.clientY;
});

function notices() {
  for (const text of state.client.palette_notices()) toast(null, text, 2600);
}

function after(changed = true) {
  flush();
  notices();
  if (changed) render();
}

function pill(value) {
  if (value.kind === "mode") {
    return element(
      "span",
      { "data-penna": "badge", class: "zz-palette-pill" },
      element("span", { class: "zz-palette-pill-prefix", "data-text": true, text: value.prefix }),
      element("span", { "data-text": true, text: value.label }),
    );
  }
  if (value.kind === "host") {
    return element(
      "span",
      { "data-penna": "badge", class: "zz-palette-pill" },
      element("span", { class: "zz-status-dot", "data-status": value.status }),
      element("span", { "data-text": true, text: value.label }),
    );
  }
  if (value.kind === "place" || value.kind === "verb") {
    const node = element("span", { "data-penna": "badge", class: "zz-palette-pill", "data-kind": value.kind }, element("span", { "data-text": true, text: value.label }));
    if (value.kind === "place") node.style.flexShrink = [...value.label].length;
    return node;
  }
  return element("span", { "data-penna": "badge", class: "zz-palette-pill zz-palette-mono", "data-text": true, text: value.label });
}

function hints(list) {
  return list.map((hint) =>
    element(
      "span",
      { class: "zz-palette-hint" },
      element("span", { class: "zz-palette-keys" }, hint.keys.map((parts) => glyphs(parts))),
      element("span", { "data-text": true, text: hint.label }),
    ),
  );
}

function sectionRow(row) {
  return element("div", { class: "zz-palette-section" }, element("span", { "data-text": true, text: row.header }));
}

function unifiedRow(row, index, selected) {
  const item = element("div", {
    "data-penna": "menu-item",
    class: "zz-palette-row",
    "aria-selected": selected ? "true" : null,
  });
  item.style.paddingLeft = `${12 + row.indent}px`;
  if (row.expanded !== null || row.indent > 0) {
    const disclosure = element("span", { class: "zz-palette-disclosure" }, row.expanded === null ? null : icon(row.expanded ? "chevron-down" : "chevron-right"));
    if (row.expanded !== null) {
      disclosure.addEventListener("mousedown", (event) => {
        event.preventDefault();
        event.stopPropagation();
      });
      disclosure.addEventListener("click", (event) => {
        event.stopPropagation();
        state.client.palette_toggle(index);
        after();
      });
    }
    item.append(disclosure);
  }
  if (row.icon) item.append(element("span", { class: "zz-palette-icon" }, icon(row.icon)));
  if (row.status) item.append(element("span", { class: "zz-status-dot", "data-status": row.status }));
  const label = element("span", { class: "zz-palette-label", "data-text": true, "data-detail": Boolean(row.detail) }, row.label.map((part) => element("span", { class: part.class || null, text: part.text })));
  item.append(label);
  if (row.detail) item.append(element("span", { class: "zz-palette-detail", "aria-label": row.detail, text: row.detail }));
  if (row.right) item.append(element("span", { class: "zz-palette-right", "data-text": true, text: row.right }));
  if (row.shortcut) {
    item.append(
      element(
        "span",
        { class: "zz-palette-shortcut" },
        row.shortcut.map((parts) => element("span", { "data-penna": "keycap", "data-size": "small" }, glyphs(parts))),
      ),
    );
  }
  if (row.badge) item.append(element("span", { "data-penna": "badge", "data-size": "small", class: "zz-palette-badge zz-palette-mono", "data-text": true, text: row.badge }));
  if (row.running) item.append(element("span", { class: "zz-status-dot", "data-status": "muted" }));
  return item;
}

function promptRow(row, index, selected) {
  const item = element("div", {
    "data-penna": "menu-item",
    class: "zz-palette-row",
    "aria-selected": selected ? "true" : null,
  });
  item.style.paddingLeft = "12px";
  item.append(element("span", { class: "zz-palette-label", "data-text": true, "data-detail": Boolean(row.detail), text: row.label[0].text }));
  if (row.detail) item.append(element("span", { class: "zz-palette-detail", "aria-label": row.detail, text: row.detail }));
  if (row.badge) item.append(element("span", { "data-penna": "badge", "data-size": "small", class: "zz-palette-badge zz-palette-mono", "data-text": true, text: row.badge }));
  return item;
}

function build(palette) {
  const unified = palette.kind === "unified";
  const input = element("input", {
    class: unified ? "" : "zz-palette-mono",
    spellcheck: "false",
    autocomplete: "off",
    autocapitalize: "off",
  });
  input.addEventListener("input", () => {
    state.client.palette_input(input.value, input.selectionStart ?? input.value.length);
    after();
  });
  input.addEventListener("select", () => {
    state.client.palette_input(input.value, input.selectionStart ?? input.value.length);
    flush();
  });
  const field = element(
    "label",
    { class: "zz-palette-input" },
    unified ? element("span", { class: "zz-palette-pills" }) : element("span", { class: "zz-palette-prompt zz-palette-mono", "data-text": true, text: palette.prompt }),
    input,
  );
  const usage = element("div", { class: "zz-palette-usage zz-palette-mono" }, element("span"));
  const rows = element("div", { class: "zz-palette-rows" });
  rows.addEventListener("mousedown", (event) => event.preventDefault());
  const footer = element("div", { "data-penna": "rule.bar", class: "zz-palette-footer" });
  const surface = element("div", { "data-penna": "palette", class: "zz-palette", "data-kind": palette.kind }, field, usage, rows, footer);
  surface.addEventListener("mousedown", (event) => {
    event.stopPropagation();
    if (event.target !== input) event.preventDefault();
  });
  const overlay = element("div", { class: "zz-palette-overlay", "data-kind": palette.kind }, surface);
  overlay.addEventListener("mousedown", (event) => {
    event.preventDefault();
    state.client.palette_dismiss();
    after();
  });
  layer.replaceChildren(overlay);
  view.node = overlay;
  view.input = input;
  view.rows = rows;
  view.usage = usage;
  view.footer = footer;
  view.field = field;
  view.kind = palette.kind;
  view.scrolled = null;
  requestAnimationFrame(() => input.focus({ preventScroll: true }));
}

function scrollTo(index) {
  const rows = view.rows;
  const child = rows.children[index];
  if (!child || view.scrolled === index) return;
  view.scrolled = index;
  const top = child.offsetTop - rows.offsetTop;
  if (top < rows.scrollTop) rows.scrollTop = top;
  else if (top + child.offsetHeight > rows.scrollTop + rows.clientHeight) rows.scrollTop = top + child.offsetHeight - rows.clientHeight;
}

function render() {
  const client = state.client;
  const json = client?.palette_view(innerHeight);
  if (!json) {
    if (view.node) {
      layer.replaceChildren();
      const hadFocus = view.node.contains(document.activeElement);
      view.node = null;
      if (hadFocus || document.activeElement === document.body) focusTerminal();
    }
    return;
  }
  const palette = JSON.parse(json);
  if (!view.node || view.kind !== palette.kind) build(palette);
  const overlay = view.node;
  if (palette.kind === "unified") overlay.style.paddingTop = `${palette.top}px`;
  const input = view.input;
  input.placeholder = palette.input.placeholder;
  if (palette.input.set || input.value !== palette.input.value) {
    input.value = palette.input.value;
    input.setSelectionRange(palette.input.cursor, palette.input.cursor);
  }
  input.readOnly = Boolean(palette.relay);
  if (palette.kind === "unified") {
    view.field.querySelector(".zz-palette-pills").replaceChildren(...palette.pills.map(pill));
    const named = palette.pills.map((value) => (value.kind === "mode" ? `${value.prefix} ${value.label}` : value.label)).join(" ");
    if (named) view.field.setAttribute("aria-label", named);
    else view.field.removeAttribute("aria-label");
  } else {
    view.field.querySelector(".zz-palette-prompt").textContent = palette.prompt;
    if (palette.prompt.trim()) view.field.setAttribute("aria-label", palette.prompt);
    else view.field.removeAttribute("aria-label");
  }
  view.usage.hidden = !palette.usage;
  const usageText = view.usage.firstChild;
  usageText.textContent = palette.usage || "";
  if (palette.usage) usageText.setAttribute("aria-label", palette.usage);
  else usageText.removeAttribute("aria-label");
  const rows = palette.rows.map((row, index) => {
    const selected = palette.selected === index;
    const node = row.header !== undefined ? sectionRow(row) : palette.kind === "unified" ? unifiedRow(row, index, selected) : promptRow(row, index, selected);
    if (row.header === undefined) {
      node.addEventListener("mousemove", (event) => {
        if (event.clientX === pointer.x && event.clientY === pointer.y) return;
        if (state.client.palette_hover(index)) after();
      });
      node.addEventListener("click", (event) => {
        state.client.palette_click(index, event.detail);
        after();
      });
    }
    return element("div", { class: "zz-palette-slot" }, node);
  });
  const empty = palette.kind === "unified" && !rows.length && Boolean(palette.empty);
  view.rows.hidden = !rows.length && !empty;
  view.rows.dataset.empty = empty;
  view.rows.style.height = empty ? "" : `${palette.listHeight}px`;
  view.rows.replaceChildren(...(empty ? [element("div", { class: "zz-palette-empty", "data-text": true, text: palette.empty })] : rows));
  view.footer.replaceChildren(...hints(palette.hints));
  if (palette.selected !== null && palette.selected !== undefined) scrollTo(palette.selected);
  else view.rows.scrollTop = 0;
}

const PALETTE_MODES = { ":": ":" };

function showPalette(mode = null) {
  const client = state.client;
  if (!client || client.palette_is_open() || !client.palette_open(PALETTE_MODES[mode] ?? null)) return;
  after();
}

on("openPalette", (event) => showPalette(event?.mode));

on("commandPrompt", () => {
  state.client.palette_prompt_changed();
  render();
});
on("chooseTree", () => {
  state.client.palette_chooser_changed();
  render();
});
on("attached", () => {
  state.client.palette_prompt_changed();
  state.client.palette_chooser_changed();
  render();
});
for (const type of ["tree", "menu", "confirm", "disconnected", "keyTables"]) on(type, () => render());
addEventListener("resize", () => render());

onKey((event) => {
  const client = state.client;
  if (!client || event.isComposing) return false;
  if (client.palette_is_open()) {
    if (["Shift", "Control", "Alt", "Meta", "CapsLock"].includes(event.key)) return true;
    if (client.palette_raw_key(...keyArgs(event))) {
      event.preventDefault();
      after();
      return true;
    }
    const selecting = view.input && view.input.selectionStart !== view.input.selectionEnd;
    const atEnd = !view.input || view.input.selectionStart === view.input.value.length;
    const name = event.key === "Tab" && event.shiftKey ? "ShiftTab" : event.key;
    const plain = !event.metaKey && !event.altKey && !event.ctrlKey;
    if (plain && ["ArrowUp", "ArrowDown", "Escape", "Tab", "ShiftTab", "Enter"].includes(name)) {
      if (view.input && view.input.value !== undefined) client.palette_input(view.input.value, view.input.selectionStart ?? view.input.value.length);
      client.palette_key(name);
      event.preventDefault();
      after();
      return true;
    }
    if (plain && (name === "ArrowLeft" || name === "ArrowRight" || name === "Backspace") && !selecting && atEnd) {
      if (client.palette_key(name)) {
        event.preventDefault();
        after();
        return true;
      }
    }
    if (event.target !== view.input) {
      view.input?.focus({ preventScroll: true });
    }
    return true;
  }
  const action = client.chrome_action(...keyArgs(event));
  if (action === "open-command-palette" && !client.daemon_overlay_open()) {
    event.preventDefault();
    showPalette();
    return true;
  }
  return false;
});

export { showPalette, render as renderPalette };
