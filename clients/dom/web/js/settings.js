import { $, command, emit, on, state } from "./core.js";
import * as preferences from "./preferences.js";
import * as engine from "../theme/zz_dom_theme.js";

const app = $(".app");
const nav = h("div", { id: "settings-nav" });
const page = h("section", { id: "settings" });
$("#tree").before(nav);
app.append(page);

const view = { section: null, advanced: false, dragging: false, pending: false, pane: 0, menu: null };
const ACCENT_TITLES = (id) => (id === "theme" ? "Theme" : id[0].toUpperCase() + id.slice(1));
const MODES = [
  ["system", "System"],
  ["light", "Light"],
  ["dark", "Dark"],
];
const PALETTE_LAYOUTS = [
  ["grouped", "Tree"],
  ["flat", "Flat"],
];
const MONO_FONTS = ["Lilex", "Menlo", "SF Mono", "Monaco", "JetBrains Mono", "Fira Code", "Cascadia Code", "Consolas", "DejaVu Sans Mono", "Ubuntu Mono", "Courier New"];
const UI_FONTS = ["Inter Variable", "SF Pro Text", "Helvetica Neue", "Arial", "Segoe UI", "Roboto", "Ubuntu", "Cantarell", ...MONO_FONTS];

function h(tag, props = {}, ...children) {
  const element = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value == null || value === false) continue;
    if (key === "class") element.className = value;
    else if (key === "text") element.textContent = value;
    else if (key === "style") for (const [name, rule] of Object.entries(value)) element.style.setProperty(name, rule);
    else if (key.startsWith("on")) element.addEventListener(key.slice(2), value);
    else element.setAttribute(key, value === true ? "" : value);
  }
  element.append(...children.flat(Infinity).filter((child) => child != null && child !== false));
  return element;
}

const icon = (name) => h("i", { "data-icon": name });
const text = (value, props = {}) => h("span", { "data-text": true, text: value, ...props });
const block = (value, props = {}) => h("div", { "aria-label": value, text: value, ...props });
const prefs = () => preferences.get();
const set = (changes) => preferences.set(changes);

function heading(title) {
  return block(title, { class: "settings-heading" });
}

function header(title) {
  return h("div", { class: "settings-header" }, block(title));
}

function entry({ title, description, glyph, actions, control, children, disabled, status }) {
  return h(
    "div",
    { class: "setting", "data-disabled": disabled },
    h(
      "div",
      { class: "setting-head" },
      h(
        "div",
        { class: "setting-copy" },
        h("div", { class: "setting-title-row" }, glyph, text(title, { class: "setting-title" }), actions),
        description && h("div", { class: "setting-description" }, block(description)),
      ),
      control && h("div", { class: "setting-control" }, control),
    ),
    status,
    children,
  );
}

function card(entries) {
  const rows = [];
  entries.filter(Boolean).forEach((row, index) => {
    if (index) rows.push(h("div", { class: "setting-rule", "data-zz": "rule.list" }));
    rows.push(row);
  });
  return h("div", { class: "settings-card", "data-zz": "settings.card", "data-ground": true }, rows);
}

function stack(title, entries) {
  return h("div", { class: "settings-stack" }, title && header(title), card(entries));
}

function button(label, props = {}, ...children) {
  return h("button", { "data-zz": "button", "aria-label": label, ...props }, ...children);
}

function quiet(label, tooltip, onclick) {
  return h("button", { "data-zz": "button.quiet", "data-size": "small", "aria-label": label, title: tooltip, onclick }, text(label));
}

function toggle(checked, label, onchange) {
  return h("button", {
    class: "zz-switch",
    role: "switch",
    "aria-checked": String(checked),
    "data-on": checked,
    onmousedown: (event) => {
      event.preventDefault();
      onchange(!checked);
    },
    onkeydown: (event) => {
      if (event.key === " " || event.key === "Enter") {
        event.preventDefault();
        onchange(!checked);
      }
    },
    "data-label": label,
  });
}

function percent(value) {
  return `${Math.round(value * 100)}%`;
}

function snap(value, min, max, step) {
  const snapped = Math.round((value - min) / step) * step + min;
  return Math.min(max, Math.max(min, Math.round(snapped * 1e6) / 1e6));
}

function slider({ label, value, min, max, step, onchange, track, commitOnRelease }) {
  const fraction = (value) => (max > min ? (value - min) / (max - min) : 0);
  const rail = h("div", { class: "zz-rail" });
  const fill = h("div", { class: "zz-fill" });
  const knob = h("div", { class: "zz-knob" });
  if (track) {
    rail.style.background = `linear-gradient(90deg, ${track.join(", ")})`;
    rail.append(knob);
  } else rail.append(fill, knob);
  const element = h("div", { class: "zz-slider", role: "slider", tabindex: 0, "aria-label": label, "aria-valuemin": min, "aria-valuemax": max }, rail);
  let current = value;
  const show = (next) => {
    current = next;
    element.style.setProperty("--zz-value", `${fraction(next) * 100}%`);
    element.setAttribute("aria-valuenow", next);
    element.dispatchEvent(new CustomEvent("shown", { detail: next }));
  };
  show(value);
  const at = (x) => {
    const bounds = element.getBoundingClientRect();
    const scale = bounds.width / element.offsetWidth || 1;
    const left = bounds.left + 8 * scale;
    const width = Math.max(1, bounds.width - 16 * scale);
    return snap(min + ((x - left) / width) * (max - min), min, max, step);
  };
  const emit = (next) => {
    if (next === current) return;
    show(next);
    if (!commitOnRelease) onchange(next);
  };
  element.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    element.focus();
    element.setPointerCapture(event.pointerId);
    view.dragging = true;
    emit(at(event.clientX));
  });
  element.addEventListener("pointermove", (event) => {
    if (element.hasPointerCapture(event.pointerId)) emit(at(event.clientX));
  });
  const release = (event) => {
    if (!element.hasPointerCapture(event.pointerId)) return;
    element.releasePointerCapture(event.pointerId);
    view.dragging = false;
    if (commitOnRelease && current !== value) onchange(current);
    if (view.pending) render();
  };
  element.addEventListener("pointerup", release);
  element.addEventListener("pointercancel", release);
  element.addEventListener("keydown", (event) => {
    const steps = event.shiftKey ? 10 : 1;
    const next = {
      ArrowLeft: current - step * steps,
      ArrowDown: current - step * steps,
      ArrowRight: current + step * steps,
      ArrowUp: current + step * steps,
      Home: min,
      End: max,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    show(snap(next, min, max, step));
    onchange(current);
  });
  return element;
}

function valueSlider(options) {
  const shown = (value) => options.display(value);
  const label = h("div", { class: "shown", "aria-label": shown(options.value), text: shown(options.value) });
  const control = slider(options);
  control.addEventListener("shown", (event) => {
    label.textContent = shown(event.detail);
    label.setAttribute("aria-label", shown(event.detail));
  });
  return h("div", { class: "settings-slider-row" }, h("div", { class: "slider" }, control), h("div", { class: "trailing" }, label));
}

function closeMenu() {
  if (view.menu?.owner) delete view.menu.owner.dataset.state;
  view.menu?.remove();
  view.menu = null;
}

function choose({ value, items, placeholder, className = "", onpick }) {
  const current = items.find((item) => item.value === value);
  const trigger = h(
    "button",
    { "data-zz": "field", class: `settings-select ${className}`, "aria-haspopup": "listbox" },
    text(current?.title ?? placeholder, { style: current?.font ? { "font-family": `"${current.font}"` } : null }),
    icon("chevron-down"),
  );
  trigger.addEventListener("click", (event) => {
    event.stopPropagation();
    if (view.menu?.owner === trigger) return closeMenu();
    closeMenu();
    const bounds = trigger.getBoundingClientRect();
    const menu = h(
      "div",
      { "data-zz": "popover", class: "zz-menu settings-menu", role: "listbox" },
      items.map((item) =>
        h(
          "div",
          {
            "data-zz": "menu-item",
            role: "option",
            "aria-selected": String(item.value === value),
            "data-state": item.value === value ? "selected" : null,
            style: item.font ? { "font-family": `"${item.font}"` } : null,
            onclick: () => {
              closeMenu();
              onpick(item.value);
            },
          },
          h("span", { class: "check" }, item.value === value && icon("check")),
          h("span", { text: item.title }),
        ),
      ),
    );
    menu.owner = trigger;
    trigger.dataset.state = "focused";
    menu.style.left = `${bounds.left}px`;
    menu.style.top = `${bounds.bottom + 4}px`;
    menu.style.minWidth = `${bounds.width}px`;
    document.body.append(menu);
    view.menu = menu;
  });
  return trigger;
}

function numberField({ value, min, max, step, onchange }) {
  const input = h("input", { inputmode: "decimal", value: numberText(value), "aria-label": null });
  const commit = (commit) => {
    const parsed = Number.parseFloat(input.value);
    let next;
    if (Number.isFinite(parsed) && commit) next = Math.min(max, Math.max(min, parsed));
    else if (Number.isFinite(parsed) && parsed >= min && parsed <= max) next = parsed;
    else if (commit) next = value;
    else return;
    if (commit) input.value = numberText(next);
    value = next;
    onchange(next);
  };
  input.addEventListener("input", () => commit(false));
  input.addEventListener("blur", () => commit(true));
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") commit(true);
  });
  const stepBy = (direction) => {
    const decimals = Math.max(decimalsOf(step), decimalsOf(value));
    const next = Math.min(max, Math.max(min, Number((value + direction * step).toFixed(decimals))));
    input.value = numberText(next);
    value = next;
    onchange(next);
  };
  return h(
    "div",
    { "data-zz": "field", class: "settings-number", role: "spinbutton" },
    h("button", { "data-zz": "button.quiet", tabindex: -1, onclick: () => stepBy(-1) }, icon("minus")),
    input,
    h("button", { "data-zz": "button.quiet", tabindex: -1, onclick: () => stepBy(1) }, icon("plus")),
  );
}

function decimalsOf(value) {
  const text = String(value);
  return text.includes(".") ? text.length - text.indexOf(".") - 1 : 0;
}

function numberText(value) {
  const text = value.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
  return text === "-0" ? "0" : text;
}

function tile({ label, preview, selected, onclick }) {
  return h(
    "div",
    { class: "settings-tile", role: "option", tabindex: selected ? 0 : -1, "aria-selected": String(selected), onclick },
    h("div", { class: "halo" }, h("div", { "data-zz": "settings.tile", "data-state": selected ? "selected" : null }, preview)),
    h("div", { class: "label", "aria-label": label, text: label }),
  );
}

function paletteContents(colors) {
  return [
    h("div", { class: "side", style: { "border-color": colors.border } }),
    h(
      "div",
      { class: "main" },
      h("div", { class: "lights" }, colors.lights.map((light) => h("i", { style: { background: light } }))),
      h("div", { class: "bar", style: { width: "36px", background: colors.text } }),
      h("div", { class: "bar", style: { width: "22px", background: colors.text } }),
      h("div", { class: "pill", style: { background: colors.accent } }),
    ),
  ];
}

function split(left, right, fraction) {
  return `linear-gradient(90deg, ${left} ${fraction * 100}%, ${right} ${fraction * 100}%)`;
}

function modePreview(mode, modes) {
  const style = { "border-radius": `${adaptive(modes.radius, 56)}px` };
  if (mode !== "system") {
    const colors = modes[mode];
    return h("div", { class: "settings-preview-tile", style: { ...style, background: split(colors.sidebar, colors.background, 20 / 84) } }, paletteContents(colors));
  }
  const { light, dark } = modes;
  return h(
    "div",
    { class: "settings-preview-tile", style: { ...style, background: split(light.sidebar, dark.background, 0.5) } },
    h("div", { class: "half", style: { left: "20px", width: "22px", background: light.background } }),
    h("div", { class: "half", style: { left: "0", width: "84px", clip: "rect(0, 42px, 56px, 0)", display: "flex" } }, paletteContents(light)),
    h("div", { class: "half", style: { right: "0", width: "42px" } }, h("div", { class: "settings-preview-tile", style: { position: "absolute", right: "0", top: "0" } }, paletteContents(dark))),
  );
}

function themePreview(preview) {
  const menu = preview.menu;
  const row = (width, selected) =>
    h(
      "div",
      { style: { "border-radius": `${adaptive(menu.rowRadius, 10)}px`, background: selected ? menu.selection : null } },
      h("div", { class: "bar", style: { width: `${width}px`, background: selected ? menu.selectedText : preview.text } }),
    );
  return h(
    "div",
    { class: "settings-preview-tile theme", style: { "border-radius": `${adaptive(preview.radius, 56)}px`, background: split(preview.sidebar, preview.background, 20 / 84) } },
    h("div", { class: "side", style: { "border-color": preview.border } }),
    h(
      "div",
      { class: "main" },
      h(
        "div",
        { class: "top" },
        h("div", { class: "lights" }, preview.lights.map((light) => h("i", { style: { background: light } }))),
        h(
          "div",
          { class: "button", style: { "border-radius": `${adaptive(Math.max(menu.rowRadius, 2), 8)}px`, background: preview.button } },
          h("i", { style: { background: preview.buttonText } }),
        ),
      ),
      h(
        "div",
        {
          class: "menu",
          style: {
            "padding-left": `${menu.inset}px`,
            "padding-right": `${menu.inset}px`,
            gap: menu.inset > 0 ? "1px" : "0",
            "border-radius": `${adaptive(menu.radius, 36)}px`,
            background: menu.fill,
            border: menu.outline ? `0.5px solid ${menu.outline}` : null,
            "box-shadow": menu.shadow ? `0 3px 16px -1px ${menu.shadow}` : null,
          },
        },
        row(24, false),
        row(18, true),
        row(21, false),
      ),
    ),
  );
}

function strip(picks) {
  const tiles = h(
    "div",
    { class: "tiles", tabindex: 0 },
    h("div", { class: "edge" }),
    picks.themes.map((theme) =>
      tile({
        label: theme.name,
        preview: themePreview(theme.preview),
        selected: theme.id === picks.selected,
        onclick: () => pickTheme(theme.id),
      }),
    ),
    h("div", { class: "edge" }),
  );
  const thumb = h("div", { class: "thumb" });
  const gutter = h("div", { class: "gutter" }, thumb);
  const place = () => {
    const ratio = tiles.clientWidth / Math.max(tiles.scrollWidth, 1);
    thumb.style.width = `calc(${ratio} * (100% - 28px))`;
    thumb.style.left = `calc(2px + ${tiles.scrollLeft / Math.max(tiles.scrollWidth, 1)} * (100% - 28px))`;
  };
  tiles.addEventListener("scroll", place);
  requestAnimationFrame(place);
  tiles.addEventListener("keydown", (event) => {
    const index = picks.themes.findIndex((theme) => theme.id === picks.selected);
    const next = { ArrowLeft: index - 1, ArrowRight: index + 1 }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    if (next >= 0 && next < picks.themes.length) pickTheme(picks.themes[next].id);
  });
  return h("div", { class: "picker-strip" }, tiles, gutter);
}

function pickTheme(id) {
  preferences.themeAction("pick", id);
}

function themeSaved(outcome) {
  if (outcome.error) toast(outcome.error, "danger");
  else if (outcome.saved) toast(`Saved the theme as ${outcome.saved}`, "success");
}

function toast(message, tone) {
  const element = h("div", { "data-zz": "toast", class: "settings-toast", role: "status", "data-tone": tone }, h("span", { text: message }));
  page.append(element);
  setTimeout(() => element.remove(), 4000);
}

async function importTheme() {
  let clipboard;
  try {
    clipboard = await navigator.clipboard.readText();
  } catch (error) {
    themeSaved({ error: `Could not read the clipboard: ${error.message || error}.` });
    return;
  }
  themeSaved(preferences.themeAction("import", clipboard));
}

function adaptive(radius, size) {
  const cap = size * 0.45;
  return radius > 0 ? cap * Math.tanh(radius / cap) : 0;
}

function ringed(on, radius, swatch) {
  return h("div", { class: "settings-ring", "data-on": on, style: { "border-radius": `${adaptive(radius > 0 ? radius + 3.5 : 0, 25)}px` } }, swatch);
}

function swatches(picks) {
  const { accent } = picks;
  const radius = `${adaptive(accent.radius, 18)}px`;
  const pick = (id) => {
    set({ accent: id === "theme" ? null : id });
  };
  const swatch = (id, color) =>
    ringed(
      accent.selected === id,
      accent.radius,
      h("button", { class: "settings-swatch", "aria-label": ACCENT_TITLES(id), title: ACCENT_TITLES(id), style: { background: color, "border-radius": radius }, onclick: () => pick(id) }),
    );
  const custom = accent.custom;
  const input = h("input", { type: "color", value: custom || "#000000", tabindex: -1 });
  input.addEventListener("change", () => set({ accent: preferences.accentHex(input.value) || input.value }));
  const customSelected = accent.selected !== "theme" && !accent.named.some((named) => named.id === accent.selected);
  return h(
    "div",
    { class: "settings-swatches" },
    swatch("theme", accent.own),
    accent.named.map((named) => swatch(named.id, named.color)),
    ringed(customSelected, accent.radius, h("label", { class: "settings-swatch custom", style: { background: custom || "transparent", "border-radius": radius, position: "relative" } }, input)),
  );
}

function appearancePage() {
  const p = prefs();
  const picks = preferences.picks();
  if (!picks) return [heading("Interface"), h("div", { class: "setting-note", text: "The theme engine did not load." })];
  const modes = h(
    "div",
    { class: "settings-tiles" },
    MODES.map(([mode, title]) =>
      tile({ label: title, preview: modePreview(mode, picks.modes), selected: p.mode === mode, onclick: () => set({ mode }) }),
    ),
  );
  const selectedSaved = picks.saved;
  const actions = h(
    "div",
    { class: "setting-actions" },
    quiet("Import", "Save the theme on the clipboard and use it", importTheme),
    selectedSaved && quiet("Delete", "Delete this saved theme and use the default one", () => preferences.themeAction("delete", picks.selected)),
  );
  const hue = h(
    "div",
    { class: "settings-slider-row" },
    h(
      "div",
      { class: "slider" },
      slider({ label: "Tint hue", value: picks.tint.hue, min: 0, max: 360, step: 1, track: picks.tint.stops, onchange: (value) => set({ "tint-hue": value }) }),
    ),
    h("div", { class: "trailing" }, h("div", { class: "dot", style: { background: picks.tint.dot } })),
  );
  const rows = [
    heading("Interface"),
    group("Theme"),
    card([
      entry({ title: "Mode", control: modes }),
      entry({ title: "Themes", actions, children: [strip(picks)] }),
      entry({ title: "Accent", control: swatches(picks) }),
      entry({ title: "Tint", control: hue }),
      entry({
        title: "Tint strength",
        control: valueSlider({ label: "Tint strength", value: picks.tint.strength, min: 0, max: 1, step: 0.01, display: percent, onchange: (value) => set({ tint: value }) }),
      }),
    ]),
    group("Sidebar"),
    card([
      entry({
        title: "Program colors",
        description: "Fill sidebar tiles with the running program's colors",
        control: toggle(p.program_colors, "Program colors", (value) => set({ program_colors: value })),
      }),
    ]),
    disclosure(),
  ];
  if (view.advanced) {
    rows.push(
      card([
        entry({
          title: "Corner radius",
          control: valueSlider({ label: "Corner radius", value: picks.radius, min: 0, max: picks.limits.radius, step: 1, display: (value) => `${Math.round(value)}px`, onchange: (value) => set({ radius: value }) }),
        }),
        entry({
          title: "Contrast",
          control: valueSlider({ label: "Contrast", value: p.contrast, min: 1, max: 2, step: 0.05, display: percent, onchange: (value) => set({ contrast: value }) }),
        }),
        entry({
          title: "UI font",
          control: choose({
            value: picks.font,
            placeholder: "System default",
            items: [{ title: "System default", value: "" }, ...availableFonts(UI_FONTS).map((font) => ({ title: font, value: font, font }))],
            onpick: (value) => set({ ui_font_family: value }),
          }),
        }),
        entry({
          title: "UI zoom",
          control: valueSlider({ label: "UI zoom", value: p.zoom, min: 0.5, max: 3, step: 0.05, display: percent, commitOnRelease: true, onchange: (value) => set({ zoom: value }) }),
        }),
        entry({ title: "Animations", control: toggle(p.animations, "Animations", (value) => set({ animations: value })) }),
        entry({
          title: "Animation speed",
          control: valueSlider({
            label: "Animation speed",
            value: p.animation_speed,
            min: picks.limits.animation[0],
            max: picks.limits.animation[1],
            step: 0.05,
            display: percent,
            onchange: (value) => set({ animation_speed: value }),
          }),
        }),
      ]),
    );
  }
  return rows.map((row) => h("div", { class: "ends" }, row));
}

function group(title) {
  return h("div", { class: "settings-group" }, header(title));
}

function disclosure() {
  return h(
    "div",
    {
      class: "settings-disclosure",
      role: "button",
      tabindex: 0,
      "aria-expanded": String(view.advanced),
      onclick: () => {
        view.advanced = !view.advanced;
        render();
      },
    },
    h("div", { class: "chevron" }, icon("chevron-right")),
    h("div", { class: "settings-header" }, block("Advanced")),
  );
}

function statusBarPage() {
  const p = prefs();
  const tab = (name, glyph, active, badges, stacked) =>
    h(
      "div",
      { "data-zz": "titlebar.window-tab", "aria-selected": String(active) },
      h("span", { class: `tile${stacked ? " stacked" : ""}`, "data-zz": "icon-tile" }, icon(glyph)),
      h("span", { class: "name", "aria-label": name, text: name }),
      h("span", { class: "badges" }, badges && [h("i", { style: { background: "var(--zz-warning)" } }), h("i", { style: { background: "var(--zz-success)" } })]),
      h("button", { "data-zz": "sidebar.row-action", title: "Close window" }, icon("xmark")),
    );
  const preview = h(
    "div",
    { class: "settings-well", "data-zz": "settings.preview", style: { "--zz-fill": "var(--zz-bg)" } },
    h(
      "div",
      { class: "settings-statusbar", "data-zz": "titlebar" },
      tab("workspace", "globe", true, false, true),
      tab("logs", "square-terminal", false, p.status_badges, false),
      !p.gaps && h("div", { class: "rule", "data-zz": "rule.bar" }),
    ),
  );
  return [
    h("div", { class: "settings-section" }, heading("Status bar"), header("Preview"), preview),
    stack(null, [
      entry({ title: "Sidebar", control: toggle(p.sidebar, "Sidebar", (value) => set({ sidebar: value })) }),
      entry({
        title: "Window badges",
        description: "Bell and activity markers on window items",
        control: toggle(p.status_badges, "Window badges", (value) => set({ status_badges: value })),
      }),
    ]),
  ];
}

const SHELL = [
  ["muted", "~/workspace"],
  ["plain", "$ cargo run"],
  ["muted", "   Compiling workspace v0.1.0"],
  ["success", "    Finished in 0.42s"],
  ["plain", "Hello, workspace."],
  ["prompt", "$"],
];
const CLAUDE = [
  ["muted", "~/workspace"],
  ["plain", "> Fix the flaky drag test"],
  ["muted", "• Bash(cargo test drag)"],
  ["success", "  └ 9 passed; 0 failed"],
  ["prompt", ">"],
];

function lines(sample) {
  const color = { muted: "var(--zz-muted)", success: "var(--zz-success)" };
  return h(
    "div",
    { class: "lines" },
    sample.map(([tone, value]) =>
      tone === "prompt"
        ? h("div", { class: "prompt" }, text(value))
        : h("div", { "aria-label": value, text: value, style: color[tone] ? { color: color[tone] } : null }),
    ),
  );
}

function headerTitle(glyph, title) {
  return h("div", { "data-zz": "pane.header.title" }, icon(glyph), text(title), h("button", { "data-zz": "pane.header.button", "data-action": "close", title: "Close pane" }, icon("xmark")));
}

function previewPane(index, body) {
  const active = view.pane === index;
  const pane = h(
    "section",
    {
      "data-zz": "pane",
      class: "preview-pane",
      "data-state": active ? "current" : null,
      "data-active": active,
      onclick: () => {
        if (view.pane !== index) {
          view.pane = index;
          render();
        }
      },
    },
    body,
  );
  pane.style.setProperty("--dim", active ? "1" : String(Math.min(1, Math.max(0, prefs().pane_inactive_opacity))));
  return pane;
}

function terminalSample(title, sample, lens) {
  return [
    h(
      "div",
      { "data-zz": "pane.header", class: "dim" },
      headerTitle("square-terminal", title),
      h("span", { class: "grow" }),
      lens && h("span", { class: "lens" }, h("button", { "data-zz": "pane.header.button", class: "action", title: "Show agent view" }, icon("bot"))),
      h("button", { "data-zz": "pane.header.button", class: "action", title: "Split bottom" }, icon("panel-bottom")),
      h("button", { "data-zz": "pane.header.button", class: "action", title: "Split right" }, icon("panel-right")),
      h("button", { "data-zz": "pane.header.button", class: "action", title: "Float" }, icon("picture-in-picture")),
    ),
    h("div", { class: "dim body" }, lines(sample)),
  ];
}

function browserSample() {
  return [
    h(
      "div",
      { "data-zz": "pane.header", class: "dim browser-header" },
      h(
        "div",
        { class: "tab-row" },
        headerTitle("globe", "Browser"),
        h("div", { "data-zz": "browser.tab", "aria-selected": "true" }, h("span", { class: "favicon" }, icon("globe")), h("span", { class: "label", "aria-label": "zed.dev", text: "zed.dev" })),
        h("button", { "data-zz": "browser.button", title: "New tab" }, icon("plus")),
      ),
      h(
        "div",
        { class: "toolbar" },
        h("button", { "data-zz": "browser.button", title: "Back" }, icon("arrow-left")),
        h("button", { "data-zz": "browser.button", title: "Forward", disabled: true }, icon("arrow-right")),
        h("button", { "data-zz": "browser.button", title: "Reload" }, icon("redo-2")),
        h("div", { class: "address", "data-zz": "browser.address" }, h("button", { "data-zz": "browser.button", class: "site", title: "Site controls" }, icon("site-controls")), h("span", { text: "zed.dev/docs" })),
        h("button", { "data-zz": "browser.button", title: "Pick an element" }, icon("inspector")),
        h("button", { "data-zz": "browser.button", title: "More browser actions" }, icon("ellipsis-vertical")),
      ),
      h("div", { class: "rule", "data-zz": "rule.bar" }),
    ),
    h(
      "div",
      { class: "page" },
      h("div", { class: "headline", "aria-label": "Key bindings", text: "Key bindings" }),
      h("div", { class: "muted", "aria-label": "Zed has a very customizable key binding system.", text: "Zed has a very customizable key binding system." }),
    ),
  ];
}

function panesPreview() {
  const p = prefs();
  const margin = p.gaps ? p.pane_margin : 0;
  const rule = (lit) => (p.gaps ? h("div", { class: "gap", style: { width: `${margin}px`, height: `${margin}px` } }) : h("div", { class: "rule", "data-lit": lit }));
  const left = previewPane(0, terminalSample("cargo run", SHELL, false));
  const right = h("div", { class: "split v" }, previewPane(1, terminalSample("claude", CLAUDE, true)), rule(view.pane === 2), previewPane(2, browserSample()));
  left.style.flex = "0.55 1 0";
  right.style.flex = "0.45 1 0";
  return h(
    "div",
    {
      class: `settings-well settings-panes${p.gaps ? " gaps" : ""}`,
      "data-zz": "settings.preview",
      style: { "--zz-fill": "var(--zz-bg)", height: `max(${margin * 3 + 112}px, clamp(300px, 40vh, 380px))`, padding: `${margin}px` },
    },
    h("div", { class: "split h" }, left, rule(view.pane === 0 || view.pane === 1), right),
  );
}

const PANE_CONTROLS = [
  { key: "pip_background_opacity", title: "PiP background opacity", min: 0, max: 1, step: 0.05, scale: 100 },
  { key: "pane_inactive_opacity", title: "Inactive pane opacity", min: 0, max: 1, step: 0.05, scale: 1 },
  { key: "pane_glow_strength", title: "Selected pane contrast", min: 0, max: 2, step: 0.05, scale: 100, gaps: true },
  { key: "pane_margin", title: "Pane margin", min: 0, max: 32, step: 0.5, scale: 1, gaps: true },
];

function paneSetting(control) {
  const p = prefs();
  const off = control.gaps && !p.gaps;
  const description = off ? "Needs pane gaps" : control.key === "pane_inactive_opacity" ? "1 turns dimming off" : null;
  const scale = control.scale;
  return entry({
    title: control.title,
    description,
    disabled: off,
    control: numberField({
      value: Math.round(p[control.key] * scale * 1e4) / 1e4,
      min: control.min * scale,
      max: control.max * scale,
      step: control.step * scale,
      onchange: (value) => set({ [control.key]: value / scale }),
    }),
  });
}

function panesPage() {
  const p = prefs();
  const [pip, inactive, glow, margin] = PANE_CONTROLS.map(paneSetting);
  return [
    h("div", { class: "settings-section" }, heading("Panes"), header("Preview"), panesPreview()),
    stack("Layout", [entry({ title: "Pane gaps", control: toggle(p.gaps, "Pane gaps", (value) => set({ gaps: value })) })]),
    stack("PiP", [pip]),
    stack("Focus", [inactive, glow]),
    stack("Frame", [margin]),
  ];
}

function terminalAppearance() {
  try {
    return state.client ? JSON.parse(state.client.appearance()) : null;
  } catch {
    return null;
  }
}

function measureCell(family, size) {
  const probe = h("span", { class: "probe", text: "m".repeat(10), style: { font: `${size}px ${family}`, "line-height": "normal" } });
  document.body.append(probe);
  const box = probe.getBoundingClientRect();
  probe.remove();
  return Math.ceil(box.height) || Math.ceil(size * 1.3);
}

function terminalPreview() {
  const p = prefs();
  const appearance = terminalAppearance() || {};
  const palette = appearance.palette || [];
  const foreground = appearance.foreground || "var(--zz-fg)";
  const fonts = [p.terminal_font_family, ...(appearance.fonts || []), "Lilex"].filter(Boolean).map((font) => `"${font}"`);
  const family = [...fonts, "ui-monospace", "monospace"].join(", ");
  const size = (appearance.size || 13) * p.terminal_font_scale;
  const cell = measureCell(family, size);
  const [top, right, bottom, left] = appearance.padding || [2, 4, 2, 4];
  const rows = Array.from({ length: 8 }, () => []);
  const write = (row, column, value, style = {}) => rows[row].push({ column, value, ...style });
  write(0, 0, "~/projects/zz", { color: palette[4] });
  write(0, 15, "main", { color: palette[2] });
  write(1, 0, ">", { color: palette[2] });
  write(1, 2, "printf 'hello, terminal\\n'");
  write(2, 0, "hello, terminal", { bold: true });
  write(3, 0, "Regular");
  write(3, 10, "Bold", { bold: true });
  write(3, 17, "Italic", { italic: true });
  for (let index = 0; index < 8; index++) {
    write(5, index * 4, "██", { color: palette[index] });
    write(6, index * 4, "██", { color: palette[index + 8] });
  }
  write(7, 0, ">", { color: palette[2] });
  const line = (cells, row) => {
    const element = h("div");
    let column = 0;
    for (const span of cells.sort((a, b) => a.column - b.column)) {
      if (span.column > column) element.append(" ".repeat(span.column - column));
      element.append(
        h("span", {
          text: span.value,
          style: { color: span.color || null, "font-weight": span.bold ? "700" : null, "font-style": span.italic ? "italic" : null },
        }),
      );
      column = span.column + span.value.length;
    }
    if (row === 7) {
      element.append(" ".repeat(2 - column));
      element.append(h("span", { class: "cursor", text: " ", style: { background: appearance.cursor || foreground } }));
    }
    return element;
  };
  const height = Math.max(190, cell * 8 + top + bottom + 2);
  return h(
    "div",
    { class: "settings-well", "data-zz": "settings.preview", style: { "--zz-fill": appearance.background || "var(--zz-bg)", height: `${height}px` } },
    h(
      "pre",
      {
        class: "settings-terminal",
        style: {
          padding: `${top}px ${right}px ${bottom}px ${left}px`,
          font: `${size}px ${family}`,
          "line-height": `${cell}px`,
          "--cell-height": `${cell}px`,
          color: foreground,
        },
      },
      rows.map(line),
    ),
  );
}

function availableFonts(candidates) {
  const context = document.createElement("canvas").getContext("2d");
  const sample = "mmmmmmmmmmlli10OQ";
  const width = (font) => {
    context.font = `32px ${font}`;
    return context.measureText(sample).width;
  };
  const bases = ["monospace", "serif", "sans-serif"].map((base) => [base, width(base)]);
  return candidates.filter((font) => bases.some(([base, size]) => width(`"${font}", ${base}`) !== size));
}

function terminalPage() {
  const p = prefs();
  return [
    heading("Terminal"),
    h("div", { class: "settings-group" }, header("Preview")),
    terminalPreview(),
    stack("Display", [
      entry({
        title: "Font",
        control: choose({
          value: p.terminal_font_family || "",
          placeholder: "Follow host",
          items: [{ title: "Follow host", value: "" }, ...availableFonts(MONO_FONTS).map((font) => ({ title: font, value: font, font }))],
          onpick: (value) => set({ terminal_font_family: value || null }),
        }),
      }),
      entry({
        title: "Font scale",
        control: valueSlider({ label: "Font scale", value: p.terminal_font_scale, min: 0.5, max: 3, step: 0.05, display: percent, onchange: (value) => set({ terminal_font_scale: value }) }),
      }),
    ]),
    stack("Host configuration", [
      entry({
        title: "Colors, cursor, and spacing",
        description: "Set in the host's Ghostty configuration",
        control: text("Shared", { class: "setting-note" }),
      }),
    ]),
  ];
}

function hostsPage() {
  const connected = state.connected;
  const endpoint = `zz web gateway at ${location.host}`;
  return [
    heading("Hosts"),
    stack("Hosts", [
      entry({
        title: "This computer",
        description: endpoint,
        glyph: h("i", { "data-icon": "hard-drive", style: { color: connected ? "var(--zz-success)" : "var(--zz-warning)" } }),
        status: h("div", { class: "setting-status" }, block(connected ? "Connected" : "Connecting…")),
        control: button("Reconnect", { onclick: () => state.socket?.close() }, text("Reconnect")),
      }),
    ]),
  ];
}

function providerEntry(row) {
  const action = row.action;
  const label = action?.label;
  const run = () => {
    if (action.copy) navigator.clipboard.writeText(action.copy).catch(() => {});
    else command("providers", ["-u", row.provider]);
  };
  return entry({
    title: row.label,
    description: row.description,
    glyph: icon(row.icon),
    control: h(
      "div",
      { class: "setting-actions provider" },
      h("div", { class: "setting-value provider-version" }, row.check && h("i", { "data-icon": "check", style: { color: "var(--zz-success)" } }), row.version && text(row.version)),
      action &&
        button(label, { "data-tone": action.primary ? "primary" : null, "data-prominent": action.primary, disabled: action.disabled, onclick: run }, action.icon && icon(action.icon), text(label)),
    ),
    children: row.lines.map((line) => h("div", { class: "setting-value provider-line", "aria-label": line.text, text: line.text, style: line.tone === "warning" ? { color: "var(--zz-warning)" } : line.tone === "plain" ? { color: "var(--zz-fg)" } : null })),
  });
}

function agentsPage() {
  const { checking, rows } = JSON.parse(engine.providers(state.client ? state.client.providers() : "null"));
  const entries = checking ? [entry({ title: "Checking", control: h("span", { class: "zz-spinner", role: "progressbar" }) })] : rows.map(providerEntry);
  return [
    heading("Agents"),
    stack("This computer", entries),
    h("div", { class: "settings-actions-end" }, button("Check again", { onclick: () => command("providers") }, text("Check again"))),
  ];
}

function systemPage() {
  const p = prefs();
  return [
    heading("System"),
    stack("Command palette", [
      entry({
        title: "Navigation layout",
        control: choose({
          value: p.palette_grouped ? "grouped" : "flat",
          className: "choice",
          items: PALETTE_LAYOUTS.map(([value, title]) => ({ value, title })),
          onpick: (value) => set({ palette_grouped: value === "grouped" }),
        }),
      }),
      entry({ title: "Command shortcuts", control: toggle(p.palette_show_keys, "Command shortcuts", (value) => set({ palette_show_keys: value })) }),
    ]),
  ];
}

function aboutPage() {
  const platform = "browser · wasm32";
  const about = preferences.about(platform);
  const logo = `img/${about.development ? "zz-dev" : "zz"}-${preferences.dark() ? "dark" : "light"}-512.png`;
  const link = (label, url) => button(label, { onclick: () => window.open(url, "_blank", "noopener") }, icon("external-link"), text(label));
  const copy = h(
    "button",
    {
      "data-zz": "button.quiet",
      class: "settings-copy",
      "data-size": "small",
      title: "Copy build information",
      onclick: async () => {
        try {
          await navigator.clipboard.writeText(about.buildInfo);
          toast("Copied build information", "success");
        } catch (error) {
          toast(String(error.message || error), "danger");
        }
      },
    },
    icon("copy"),
  );
  return [
    h(
      "div",
      { class: "settings-hero" },
      h("img", { src: logo, alt: "" }),
      h(
        "div",
        { class: "copy" },
        h("div", { class: "name", "aria-label": "zz", text: "zz" }),
        h("div", { class: "tagline", "aria-label": "tmux, ghostty and ala walked into a mux.", text: "tmux, ghostty and ala walked into a mux." }),
        h("span", { "data-zz": "badge" }, text(`v${about.version}`)),
      ),
    ),
    stack("Build", [
      entry({ title: "Version", actions: copy, control: text(about.version, { class: "setting-value" }) }),
      entry({ title: "Platform", control: text(platform, { class: "setting-value" }) }),
    ]),
    stack("Project", [
      entry({ title: "Source code", control: link("GitHub", about.repository) }),
      entry({ title: "Releases", control: link("Releases", about.releases) }),
      entry({ title: "Report an issue", control: link("New issue", about.issues) }),
      entry({ title: "License", control: text("MIT or Apache-2.0", { class: "setting-value" }) }),
    ]),
  ];
}

const PAGES = {
  appearance: appearancePage,
  statusbar: statusBarPage,
  panes: panesPage,
  terminal: terminalPage,
  hosts: hostsPage,
  agents: agentsPage,
  advanced: systemPage,
  about: aboutPage,
};

function renderNav() {
  const row = (title, glyph, selected, onclick) =>
    h("button", { "data-zz": "sidebar.row", title, "data-state": selected ? "selected" : null, onclick }, icon(glyph), text(title), h("span", { class: "grow" }));
  const rows = [h("div", { class: "back" }, row("Back", "arrow-left", false, close))];
  let previous = null;
  for (const section of preferences.sections()) {
    if (previous !== null && previous !== section.group) rows.push(h("div", { class: "gap" }));
    previous = section.group;
    rows.push(row(section.title, section.icon, section.id === view.section, () => open(section.id)));
  }
  nav.replaceChildren(...rows);
}

function render() {
  if (!view.section) return;
  if (view.dragging || page.querySelector("input:focus")) {
    view.pending = true;
    return;
  }
  view.pending = false;
  closeMenu();
  const scroller = page.querySelector(".settings-scroll");
  const offset = scroller?.dataset.section === view.section ? scroller.scrollTop : 0;
  const strip = page.querySelector(".picker-strip > .tiles")?.scrollLeft || 0;
  renderNav();
  const list = view.section === "appearance";
  const content = h("div", { class: `settings-content${list ? " list" : ""}` }, PAGES[view.section]());
  const next = h("div", { class: "settings-scroll", "data-section": view.section }, content);
  page.replaceChildren(next);
  next.scrollTop = offset;
  const tiles = page.querySelector(".picker-strip > .tiles");
  if (tiles) tiles.scrollLeft = strip;
}

export function open(section = "appearance") {
  const known = preferences.sections().some((entry) => entry.id === section);
  view.section = known ? section : "appearance";
  app.dataset.route = "settings";
  document.activeElement?.blur?.();
  if (view.section === "agents" && state.client?.providers() === "null") command("providers");
  render();
  emit("settings", { open: true, section: view.section });
}

export function close() {
  if (!view.section) return;
  closeMenu();
  view.section = null;
  delete app.dataset.route;
  page.replaceChildren();
  emit("settings", { open: false });
}

export function isOpen() {
  return view.section !== null;
}

$("#open-settings").addEventListener("click", () => open("appearance"));

window.addEventListener(
  "keydown",
  (event) => {
    if (!view.section || event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    if (view.menu) closeMenu();
    else close();
  },
  true,
);

document.addEventListener("pointerdown", (event) => {
  if (view.menu && !view.menu.contains(event.target) && event.target !== view.menu.owner) closeMenu();
});

page.addEventListener("focusout", () => {
  if (view.pending) setTimeout(render);
});

preferences.subscribe(render);
on("providers", () => view.section === "agents" && render());
on("appearance", () => view.section === "terminal" && render());
on("attached", () => (view.section === "hosts" || view.section === "agents") && render());
on("disconnected", () => view.section === "hosts" && render());
on("open-settings", ({ section }) => open(section));

globalThis.zzSettings = { open, close, isOpen };
