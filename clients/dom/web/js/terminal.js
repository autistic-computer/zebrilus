import { emit, flush, markPane, on, state } from "./core.js";
import { panes, paneRenderer } from "./workspace.js";

const THUMB_WIDTH = 6;
const THUMB_INSET = 2;
const MIN_THUMB = 48;
const PREVIEW_SIDE = 300;
const AUTOSCROLL_MS = 33;
const HOVER_DELAY_MS = 250;
const SCROLL_DEBOUNCE_MS = 120;
const SCROLL_TIMEOUT_MS = 2000;
const BACKFILL_QUIET_MS = 100;

export const views = new Map();
let ime = null;
const metricsCache = new Map();

export function bindIme(element) {
  ime = element;
}

export function padding() {
  return state.appearance?.padding || [2, 4, 2, 4];
}

const scale = () => devicePixelRatio || 1;
const snap = (value) => Math.round(value * scale()) / scale();

function family() {
  const fonts = (state.appearance?.fonts || []).map((font) => `"${font}"`);
  return [...fonts, '"Lilex"', "ui-monospace", "monospace"].join(", ");
}

function metrics(size) {
  const key = `${size}|${family()}|${scale()}`;
  if (metricsCache.has(key)) return metricsCache.get(key);
  const probe = document.createElement("span");
  probe.className = "probe";
  probe.style.font = `${state.appearance?.weight || 400} ${size}px ${family()}`;
  probe.style.lineHeight = "normal";
  probe.textContent = "m".repeat(100);
  document.body.append(probe);
  const box = probe.getBoundingClientRect();
  probe.remove();
  const device = 1 / scale();
  const advance = box.width / 100 > 1 ? box.width / 100 : 8;
  const width = Math.max(snap(Math.max(advance, device)), device);
  const height = state.client ? state.client.terminal_metrics(box.height, scale()) : snap(box.height);
  const result = { size, width, height, advance };
  metricsCache.set(key, result);
  return result;
}

function measure() {
  if (!state.client) return;
  state.appearance = JSON.parse(state.client.appearance());
  metricsCache.clear();
  const root = document.documentElement.style;
  const [top, right, bottom, left] = padding();
  root.setProperty("--term-font", family());
  root.setProperty("--term-weight", String(state.appearance?.weight || 400));
  root.setProperty("--term-padding", `${top}px ${right}px ${bottom}px ${left}px`);
  const cell = metrics(state.appearance?.size || 13);
  state.cell = { width: cell.width, height: cell.height };
  for (const view of views.values()) {
    view.size = null;
    view.resetBlink();
    markPane(view.pane.id);
  }
}

function escape(text) {
  return text.replace(/[&<>]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[char]);
}

function rgb(hex, alpha) {
  const value = parseInt(hex.slice(1), 16);
  return `rgb(${(value >> 16) & 255} ${(value >> 8) & 255} ${value & 255} / ${alpha})`;
}

function textHtml(line, styles) {
  if (!line || line.shimmer) return "";
  return line.runs
    .map(([text, style, wide]) => {
      const rules = styles[style];
      if (!rules && !wide) return escape(text);
      return `<span${wide ? ' class="w"' : ""}${rules ? ` style="${rules}"` : ""}>${escape(text)}</span>`;
    })
    .join("");
}

function backgroundHtml(line, foreground) {
  if (!line) return "";
  if (line.shimmer) return `<span style="left:0;width:100%;background:${rgb(foreground, 0.04)}"></span>`;
  return line.bg
    .map(
      ([start, count, color]) =>
        `<span style="left:calc(var(--cw) * ${start});width:calc(var(--cw) * ${count});background:${color}"></span>`,
    )
    .join("");
}

function mods(event) {
  return [event.shiftKey, event.ctrlKey, event.altKey, event.metaKey];
}

function pressedButton(buttons) {
  if (buttons & 1) return 0;
  if (buttons & 4) return 1;
  if (buttons & 2) return 2;
  return -1;
}

function pasteError(text) {
  emit("clientMessage", { pane: null, kind: "Error", text, durationMs: null, messageId: null });
}

function chip(icon, label, detail) {
  const element = document.createElement("div");
  element.dataset.zz = "pane.chip";
  element.className = "badge";
  element.innerHTML = `<i data-icon="${icon}"></i><span class="badge-label"></span>`;
  const holder = element.querySelector(".badge-label");
  if (label) {
    const text = document.createElement("span");
    text.dataset.text = "";
    text.textContent = label;
    holder.append(text);
  }
  if (detail) {
    const text = document.createElement("span");
    text.className = "detail";
    text.dataset.text = "";
    text.textContent = detail;
    holder.append(text);
  }
  return element;
}

function line(text, quiet) {
  const element = document.createElement("div");
  element.dataset.zz = "pane.chip";
  element.className = "chip-line";
  element.toggleAttribute("data-quiet", quiet);
  element.innerHTML = `<span data-text></span>`;
  element.firstChild.textContent = text;
  return element;
}

function terminal(pane) {
  pane.body.innerHTML = [
    '<div class="term"><div class="surface"><div class="grid"><div class="content">',
    '<div class="kitty" data-layer="bg"></div><div class="bgs"></div><div class="kitty" data-layer="below"></div>',
    '<div class="ovl"></div><div class="rows"></div><div class="kitty" data-layer="above"></div>',
    '<div class="cursor" hidden></div><div class="compose" hidden></div>',
    '</div></div><div class="thumb" hidden></div></div>',
    '<div class="stack tr"></div><div class="stack br"></div></div>',
  ].join("");
  const term = pane.body.querySelector(".term");
  const $ = (selector) => term.querySelector(selector);
  const view = {
    pane,
    term,
    output: false,
    size: null,
    generation: -1,
    viewport: null,
    geometry: null,
    fontDelta: 0,
    focused: false,
    composing: "",
    pointer: null,
    dragging: false,
    autoscroll: 0,
    searchShown: false,
    hover: { image: null, ready: false, timer: 0 },
    blink: { visible: true, timer: 0 },
    text: new Map(),
    backgrounds: new Map(),
    overlayHtml: "",
    stacks: "",
    images: new Map(),
    find: null,
    get id() {
      return pane.id;
    },
    metrics() {
      const base = state.appearance?.size || 13;
      return metrics(Math.min(48, Math.max(7, base + view.fontDelta)));
    },
    measureGrid() {
      const cell = view.metrics();
      const surface = $(".surface");
      const width = surface.clientWidth;
      const height = surface.clientHeight;
      const tolerance = (1 / scale()) * 0.01;
      const columns = Math.max(1, Math.floor((width + tolerance) / cell.width));
      const rows = Math.max(1, Math.floor((height + tolerance) / cell.height));
      return { cell, width, height, columns, rows };
    },
    applyGeometry(json) {
      const grid = view.measureGrid();
      const { cell, columns, rows, height } = grid;
      const spare = Math.max(0, height - rows * cell.height);
      const top = json?.bottomAnchored ? snap(spare) : 0;
      const shift = json?.subRow > 0 ? snap(Math.min(json.subRow, cell.height)) : 0;
      view.geometry = { ...grid, top, shift };
      const style = term.style;
      style.setProperty("--cw", `${cell.width}px`);
      style.setProperty("--lh", `${cell.height}px`);
      style.setProperty("--ls", `${cell.width - cell.advance}px`);
      style.setProperty("--term-size", `${cell.size}px`);
      style.setProperty("--term-hair", `${1 / scale()}px`);
      const gridElement = $(".grid");
      gridElement.style.top = `${top}px`;
      gridElement.style.width = `${columns * cell.width}px`;
      gridElement.style.height = `${rows * cell.height}px`;
      $(".content").style.transform = shift ? `translateY(${shift}px)` : "";
    },
    sync() {
      const geometry = view.geometry;
      if (!geometry || !state.client) return;
      const rect = $(".surface").getBoundingClientRect();
      const { cell, columns, rows, top, shift } = geometry;
      state.client.terminal_geometry(
        pane.id,
        view.output,
        new Float32Array([
          rect.left,
          rect.top + top,
          columns * cell.width,
          rows * cell.height,
          cell.width,
          cell.height,
          shift,
          scale(),
          columns,
          rows,
        ]),
        new Float32Array([rect.left, rect.top, rect.width, rect.height]),
      );
    },
    update() {
      const client = state.client;
      if (!client) return;
      view.output = client.command_output_pane() === pane.id;
      term.toggleAttribute("data-output", view.output);
      const grid = view.measureGrid();
      const compose = $(".compose");
      let hidden = 0;
      if (view.composing) {
        compose.textContent = view.composing;
        hidden = Math.max(1, Math.ceil(compose.scrollWidth / grid.cell.width));
      }
      const json = client.viewport(pane.id, view.output, grid.rows, hidden);
      if (!json) return;
      const viewport = JSON.parse(json);
      view.viewport = viewport;
      view.applyGeometry(viewport);
      term.style.setProperty("--term-ink", rgb(viewport.foreground, "var(--term-alpha, 1)"));
      view.drawRows(viewport);
      view.drawOverlays(viewport);
      view.drawKitty(viewport);
      view.drawThumb(viewport);
      view.drawStacks(viewport);
      view.drawFind(viewport);
      view.drawPreview(viewport);
      term.toggleAttribute("data-link", Boolean(viewport.hovered));
      view.syncFocus();
      view.drawCursor();
      view.sync();
    },
    drawRows(viewport) {
      const textLayer = $(".rows");
      const backgroundLayer = $(".bgs");
      const wanted = new Set();
      const place = (index, line) => {
        wanted.add(index);
        for (const [layer, cache, html] of [
          [textLayer, view.text, textHtml(line, viewport.styles)],
          [backgroundLayer, view.backgrounds, backgroundHtml(line, viewport.foreground)],
        ]) {
          let entry = cache.get(index);
          if (!entry) {
            const element = document.createElement("div");
            element.style.setProperty("--i", index);
            layer.append(element);
            entry = { element, html: null };
            cache.set(index, entry);
          }
          if (entry.html !== html) {
            entry.html = html;
            entry.element.innerHTML = html;
          }
        }
      };
      viewport.lines.forEach((line, index) => place(index, line));
      if (viewport.above) place(-1, viewport.above);
      for (const cache of [view.text, view.backgrounds]) {
        for (const [index, entry] of cache) {
          if (!wanted.has(index)) {
            entry.element.remove();
            cache.delete(index);
          }
        }
      }
    },
    drawOverlays(viewport) {
      const radius = `${3 / scale()}px`;
      const hair = `${1 / scale()}px`;
      const html = viewport.overlays
        .map((span) => {
          const end = span.end < 0 ? viewport.columns : span.end;
          const corners = span.corners
            ? `border-radius:${span.corners.map((round) => (round ? radius : "0")).join(" ")};`
            : "";
          const underline = span.underline
            ? `<i style="position:absolute;left:0;right:0;bottom:0;height:${hair};background:${span.underline}"></i>`
            : "";
          return `<div data-kind="${span.kind}" style="left:calc(var(--cw) * ${span.start});top:calc(var(--lh) * ${span.row});width:calc(var(--cw) * ${end - span.start});height:var(--lh);background:${span.color};${corners}">${underline}</div>`;
        })
        .join("");
      if (view.overlayHtml !== html) {
        view.overlayHtml = html;
        $(".ovl").innerHTML = html;
      }
    },
    imageUrl(image, generation, width, height) {
      const key = `${image}:${generation}`;
      let entry = view.images.get(key);
      if (entry) return entry.url;
      entry = { url: null };
      view.images.set(key, entry);
      const bytes = state.client.image_bytes(pane.id, image);
      if (!bytes) return null;
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      canvas.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.length), width, height), 0, 0);
      canvas.toBlob((blob) => {
        if (!blob || view.images.get(key) !== entry) return;
        entry.url = URL.createObjectURL(blob);
        markPane(pane.id);
      });
      return null;
    },
    forgetImages(ids) {
      for (const [key, entry] of view.images) {
        if (!ids || ids.includes(Number(key.split(":")[0]))) {
          if (entry.url) URL.revokeObjectURL(entry.url);
          view.images.delete(key);
        }
      }
    },
    drawKitty(viewport) {
      const { cell } = view.geometry;
      const layers = { bg: [], below: [], above: [] };
      for (const placement of viewport.kitty) {
        const url = view.imageUrl(placement.image, placement.generation, placement.width, placement.height);
        if (!url) continue;
        const left = cell.width * placement.column + placement.offsetX / scale();
        const top = cell.height * placement.row + placement.offsetY / scale();
        const width = cell.width * placement.columns;
        const height = cell.height * placement.rows;
        let inner = `left:0;top:0;width:${width}px;height:${height}px`;
        if (placement.source) {
          const [x, y, w, h] = placement.source;
          if (!w || !h || x + w > placement.width || y + h > placement.height) continue;
          const xs = width / w;
          const ys = height / h;
          inner = `left:${-x * xs}px;top:${-y * ys}px;width:${placement.width * xs}px;height:${placement.height * ys}px`;
        }
        layers[placement.layer].push(
          `<div style="left:${left}px;top:${top}px;width:${width}px;height:${height}px"><img alt="" src="${url}" style="${inner}"></div>`,
        );
      }
      for (const layer of term.querySelectorAll(".kitty")) {
        const html = layers[layer.dataset.layer].join("");
        if (layer.dataset.html !== html) {
          layer.dataset.html = html;
          layer.innerHTML = html;
        }
      }
    },
    drawThumb(viewport) {
      const thumb = $(".thumb");
      const [total, offset, len] = viewport.scrollbar;
      if (total <= len || total === 0) {
        thumb.hidden = true;
        return;
      }
      const track = view.geometry.height - THUMB_INSET * 2;
      const height = Math.max(track * (len / total), MIN_THUMB);
      const travel = Math.max(track - height, 0);
      const progress = offset / Math.max(total - len, 1);
      thumb.hidden = false;
      thumb.style.top = `${THUMB_INSET + travel * progress}px`;
      thumb.style.height = `${height}px`;
      thumb.style.width = `${THUMB_WIDTH}px`;
      thumb.style.right = `${THUMB_INSET}px`;
    },
    drawStacks(viewport) {
      const status = view.output ? null : JSON.parse(state.client.pane_status(pane.id));
      const key = JSON.stringify([viewport.mode, viewport.status, viewport.hovered?.image == null && viewport.hovered?.uri, status]);
      if (view.stacks === key) return;
      view.stacks = key;
      const top = $(".stack.tr");
      const bottom = $(".stack.br");
      for (const element of [...top.children]) if (!element.classList.contains("find")) element.remove();
      bottom.replaceChildren();
      if (viewport.mode) {
        const icon = viewport.mode.label === "Copy mode" ? "copy" : viewport.mode.label ? "file" : "arrow-down";
        top.append(chip(icon, viewport.mode.label, viewport.mode.detail));
      }
      if (status?.dead) top.append(chip("circle-x", status.dead));
      if (status?.synchronized) top.append(chip("layers", "Sync"));
      if (status?.zoomed) {
        const button = document.createElement("button");
        button.dataset.zz = "pane.chip";
        button.className = "unzoom";
        button.title = "Unzoom";
        button.innerHTML = `<i data-icon="window-restore"></i><span data-text>Unzoom</span>`;
        button.addEventListener("click", (event) => {
          event.stopPropagation();
          state.client.unzoom(pane.id);
          flush();
        });
        top.append(button);
      }
      if (viewport.hovered && viewport.hovered.image == null) bottom.append(line(viewport.hovered.uri, false));
      if (viewport.status) bottom.append(line(viewport.status, true));
    },
    buildFind() {
      const find = document.createElement("div");
      find.dataset.zz = "pane.find-bar";
      find.dataset.ground = "";
      find.className = "find";
      find.innerHTML = `
        <label data-zz="field" class="find-field"><i data-icon="search"></i><input placeholder="Find in this pane…" spellcheck="false" autocomplete="off"></label>
        <div class="find-tail">
          <span class="find-result" data-text></span>
          <button data-zz="icon-button.flat" data-size="small" data-action="regex" title="Regular expression"><i data-icon="asterisk"></i></button>
          <button data-zz="icon-button.flat" data-size="small" data-action="case" title="Smart case"><i data-icon="case-sensitive"></i></button>
          <button data-zz="icon-button.flat" data-size="small" data-action="previous" title="Previous match"><i data-icon="chevron-up"></i></button>
          <button data-zz="icon-button.flat" data-size="small" data-action="next" title="Next match"><i data-icon="chevron-down"></i></button>
          <button data-zz="icon-button.flat" data-size="small" data-action="close" title="Close search"><i data-icon="xmark"></i></button>
        </div>`;
      const input = find.querySelector("input");
      for (const type of ["mousedown", "mouseup", "wheel", "contextmenu"]) {
        find.addEventListener(type, (event) => event.stopPropagation());
      }
      find.addEventListener("click", (event) => {
        const action = event.target.closest("[data-action]")?.dataset.action;
        if (!action || event.target.closest("button")?.disabled) return;
        event.stopPropagation();
        const client = state.client;
        if (action === "regex" || action === "case") client.search_toggle(pane.id, view.output, action);
        if (action === "previous") client.search_step(pane.id, view.output, true);
        if (action === "next") client.search_step(pane.id, view.output, false);
        if (action === "close") view.closeSearch(null);
        flush();
        markPane(pane.id);
      });
      input.addEventListener("input", () => {
        state.client.search_text(pane.id, view.output, input.value);
        flush();
        markPane(pane.id);
      });
      input.addEventListener("focus", () => {
        if (!view.output) state.client.command("select-pane", ["-Z", "-t", `%${pane.id}`]);
        flush();
        markPane(pane.id);
      });
      input.addEventListener("blur", () => markPane(pane.id));
      input.addEventListener("keydown", (event) => {
        const client = state.client;
        if (event.isComposing) return;
        if (event.key === "Escape") {
          event.preventDefault();
          view.closeSearch("Escape");
        } else if (event.key === "Enter") {
          event.preventDefault();
          if (client.search_enter(pane.id, view.output, event.shiftKey)) {
            view.searchShown = false;
            focusIme();
          }
        } else if (event.altKey && /^Key[RC]$/.test(event.code)) {
          if (client.search_key(pane.id, view.output, event.code.slice(3).toLowerCase(), ...mods(event))) event.preventDefault();
        } else if ((event.metaKey || event.ctrlKey) && event.code === "KeyF") {
          event.preventDefault();
          input.select();
        }
        flush();
        markPane(pane.id);
      });
      return find;
    },
    openSearch() {
      state.client.search_open(pane.id, view.output);
      flush();
      view.searchShown = false;
      markPane(pane.id);
    },
    closeSearch(swallow) {
      state.client.search_close(pane.id, view.output, swallow);
      flush();
      view.searchShown = false;
      focusIme();
      markPane(pane.id);
    },
    searchInput() {
      return view.find?.querySelector("input") || null;
    },
    drawFind(viewport) {
      const search = viewport.search;
      if (!search) {
        view.find?.remove();
        view.searchShown = false;
        return;
      }
      if (!view.find) view.find = view.buildFind();
      const top = $(".stack.tr");
      if (top.firstChild !== view.find) top.prepend(view.find);
      const input = view.find.querySelector("input");
      if (!view.searchShown) {
        view.searchShown = true;
        input.value = search.text;
        requestAnimationFrame(() => {
          input.focus({ preventScroll: true });
          input.select();
        });
      }
      const result = view.find.querySelector(".find-result");
      result.textContent = search.result;
      result.toggleAttribute("data-invalid", search.invalid);
      const button = (action) => view.find.querySelector(`[data-action="${action}"]`);
      button("regex").dataset.state = search.regex ? "selected" : "";
      button("case").dataset.state = search.caseSensitive ? "selected" : "";
      button("case").title = search.caseLabel;
      for (const action of ["previous", "next"]) {
        button(action).disabled = !search.enabled;
        button(action).dataset.state = search.enabled ? "" : "disabled";
      }
    },
    drawPreview(viewport) {
      const number = view.pointer && !view.output ? viewport.hovered?.image ?? null : null;
      const hover = view.hover;
      if (hover.image !== number) {
        clearTimeout(hover.timer);
        hover.image = number;
        hover.ready = false;
        if (number != null) {
          state.client.fetch_pasted_image(pane.id, number);
          flush();
          hover.timer = setTimeout(() => {
            hover.ready = true;
            markPane(pane.id);
          }, HOVER_DELAY_MS);
        }
      }
      let preview = view.preview;
      const bytes = hover.ready && number != null ? state.client.pasted_image(pane.id, number) : null;
      const link = term.querySelector('.ovl [data-kind="link"]');
      if (!bytes || !link) {
        preview?.remove();
        view.preview = null;
        return;
      }
      if (!preview) {
        preview = document.createElement("div");
        preview.dataset.zz = "popover";
        preview.className = "image-preview";
        preview.innerHTML = "<img alt=''>";
        document.body.append(preview);
        view.preview = preview;
      }
      if (preview.dataset.number !== String(number)) {
        preview.dataset.number = String(number);
        const image = preview.firstChild;
        if (image.src) URL.revokeObjectURL(image.src);
        image.src = URL.createObjectURL(new Blob([bytes], { type: state.client.pasted_image_type(pane.id, number) || "" }));
      }
      const bounds = link.getBoundingClientRect();
      const margin = 8;
      const above = bounds.top >= PREVIEW_SIDE + 8;
      let left = bounds.left;
      let top = above ? bounds.top - PREVIEW_SIDE : bounds.bottom;
      left = Math.max(margin, Math.min(left, innerWidth - PREVIEW_SIDE - margin));
      top = Math.max(margin, Math.min(top, innerHeight - PREVIEW_SIDE - margin));
      preview.style.left = `${left}px`;
      preview.style.top = `${top}px`;
    },
    syncFocus() {
      const active = pane.element.hasAttribute("data-active");
      const focused = active && document.hasFocus() && document.activeElement === ime;
      if (state.client.terminal_focus(pane.id, view.output, focused)) {
        flush();
        view.focused = focused;
        if (!focused) view.stopAutoscroll();
        view.resetBlink();
      }
      view.focused = focused;
      view.ensureBlink();
    },
    ensureBlink() {
      const blink = view.blink;
      const should = view.focused && view.viewport?.cursor?.blinks;
      if (!should) {
        clearInterval(blink.timer);
        blink.timer = 0;
        blink.visible = true;
        return;
      }
      if (blink.timer) return;
      blink.timer = setInterval(() => {
        if (!(view.focused && view.viewport?.cursor?.blinks)) {
          view.ensureBlink();
          view.drawCursor();
          return;
        }
        blink.visible = !blink.visible;
        view.drawCursor();
      }, state.client.cursor_blink_interval());
    },
    resetBlink() {
      clearInterval(view.blink.timer);
      view.blink.timer = 0;
      view.blink.visible = true;
      view.ensureBlink();
      view.drawCursor();
    },
    drawCursor() {
      const mark = $(".cursor");
      const compose = $(".compose");
      const viewport = view.viewport;
      const geometry = view.geometry;
      const at = viewport?.cursor;
      compose.hidden = !view.composing || !at;
      if (!at || !geometry) {
        mark.hidden = true;
        return;
      }
      const { cell } = geometry;
      const hair = 1 / scale();
      const left = at.column * cell.width;
      const top = at.row * cell.height;
      if (view.composing) {
        compose.style.left = `${left}px`;
        compose.style.top = `${top}px`;
        compose.style.color = viewport.foreground;
        compose.style.textDecorationColor = viewport.foreground;
      }
      const focused = view.focused;
      const visible = !view.composing && (!focused || view.blink.visible);
      mark.hidden = !visible;
      if (focused && ime) {
        const rect = $(".content").getBoundingClientRect();
        ime.style.left = `${rect.left + left}px`;
        ime.style.top = `${rect.top + top}px`;
        ime.style.height = `${cell.height}px`;
      }
      if (!visible) return;
      const style = focused ? at.style : "hollow";
      let width = cell.width * at.width;
      let height = cell.height;
      let offsetTop = 0;
      mark.className = `cursor ${style}`;
      mark.textContent = "";
      mark.style.border = "";
      mark.style.background = "";
      mark.style.color = viewport.background;
      if (style === "hollow") {
        mark.style.border = `${hair}px solid ${at.color}`;
      } else if (style === "bar") {
        width = hair;
        mark.style.background = at.color;
      } else if (style === "underline") {
        offsetTop = cell.height - hair * 2;
        height = hair * 2;
        mark.style.background = at.color;
      } else {
        mark.style.background = at.color;
        mark.textContent = at.text;
        mark.style.fontWeight = at.bold ? String(Math.min(900, (state.appearance?.weight || 400) + 300)) : "";
        mark.style.fontStyle = at.italic ? "italic" : "";
        if (at.text) width = Math.max(width, mark.scrollWidth);
      }
      mark.style.left = `${left}px`;
      mark.style.top = `${top + offsetTop}px`;
      mark.style.width = `${width}px`;
      mark.style.height = `${height}px`;
    },
    compose(text) {
      view.composing = text || "";
      markPane(pane.id);
    },
    stopAutoscroll() {
      clearInterval(view.autoscroll);
      view.autoscroll = 0;
    },
    startAutoscroll() {
      if (view.autoscroll) return;
      view.autoscroll = setInterval(() => {
        if (!state.client.selection_autoscroll(pane.id, view.output)) view.stopAutoscroll();
        flush();
      }, AUTOSCROLL_MS);
    },
    step(response) {
      const result = JSON.parse(response);
      flush();
      if (result.redraw) markPane(pane.id);
      for (const generation of result.syncs || []) view.scheduleSync(generation);
      if (result.autoscroll) view.startAutoscroll();
      return result;
    },
    scheduleSync(generation) {
      setTimeout(() => {
        const sync = state.client.scroll_sync(pane.id, generation);
        flush();
        if (sync === 1) markPane(pane.id);
        if (sync === 2) {
          setTimeout(() => {
            if (state.client.scroll_expire(pane.id, generation)) markPane(pane.id);
          }, SCROLL_TIMEOUT_MS - SCROLL_DEBOUNCE_MS);
        }
      }, SCROLL_DEBOUNCE_MS);
    },
    font(delta) {
      view.fontDelta = Math.min(24, Math.max(-6, view.fontDelta + delta));
      view.size = null;
      markPane(pane.id);
    },
    layout() {
      const client = state.client;
      if (!client || !view.geometry) return;
      const { cell, columns, rows, width, height } = view.geometry;
      if (width < cell.width || height < cell.height) return;
      const size = `${view.output ? "o" : "p"}:${columns}x${rows}:${cell.width}x${cell.height}`;
      const generation = view.output ? 0 : client.layout_generation();
      if (view.size === size && view.generation === generation) return;
      view.size = size;
      view.generation = generation;
      const physicalWidth = Math.max(1, Math.round(cell.width * scale()));
      const physicalHeight = Math.max(1, Math.round(cell.height * scale()));
      if (view.output) client.resize_output(columns, rows, physicalWidth, physicalHeight);
      else client.resize(pane.id, columns, rows, physicalWidth, physicalHeight);
      flush();
    },
    destroy() {
      view.stopAutoscroll();
      clearInterval(view.blink.timer);
      clearTimeout(view.hover.timer);
      view.preview?.remove();
      view.forgetImages(null);
      views.delete(pane.id);
    },
  };

  const point = (event) => [event.clientX, event.clientY];

  const onWindowMove = (event) => {
    view.pointer = point(event);
    view.sync();
    view.step(state.client.mouse_move(pane.id, view.output, ...point(event), pressedButton(event.buttons), ...mods(event)));
  };
  const onWindowUp = (event) => {
    view.sync();
    view.step(state.client.mouse_up(pane.id, view.output, ...point(event), event.button, event.detail || 1, ...mods(event)));
    view.resetBlink();
    if (!event.buttons) {
      view.dragging = false;
      view.stopAutoscroll();
      removeEventListener("mousemove", onWindowMove, true);
      removeEventListener("mouseup", onWindowUp, true);
    }
  };

  term.addEventListener("mousedown", (event) => {
    if (event.target.closest(".stack.tr")) return;
    view.sync();
    const result = view.step(
      state.client.mouse_down(pane.id, view.output, ...point(event), event.button, event.detail || 1, ...mods(event)),
    );
    if (result.stop) event.preventDefault();
    focusIme();
    view.resetBlink();
    if (!view.dragging) {
      view.dragging = true;
      addEventListener("mousemove", onWindowMove, true);
      addEventListener("mouseup", onWindowUp, true);
    }
  });
  term.addEventListener("mousemove", (event) => {
    view.pointer = point(event);
    if (view.dragging) return;
    view.sync();
    view.step(state.client.mouse_move(pane.id, view.output, ...point(event), -1, ...mods(event)));
    if (view.viewport?.hovered?.image != null) markPane(pane.id);
  });
  term.addEventListener("mouseleave", () => {
    view.pointer = null;
    if (view.dragging) return;
    state.client.mouse_leave(pane.id, view.output);
    flush();
    markPane(pane.id);
  });
  term.addEventListener("contextmenu", (event) => {
    if (!event.target.closest(".stack")) event.preventDefault();
  });
  term.addEventListener(
    "wheel",
    (event) => {
      if (event.target.closest(".find")) return;
      event.preventDefault();
      view.sync();
      const delta = -event.deltaY;
      view.step(state.client.wheel(pane.id, view.output, delta, event.deltaMode !== 1, ...point(event), ...mods(event)));
    },
    { passive: false },
  );
  views.set(pane.id, view);
  return view;
}

function focusIme() {
  emit("focusTerminal");
}

export function activeView() {
  for (const view of views.values()) {
    if (view.pane.element.hasAttribute("data-active") && view.pane.element.isConnected) return view;
  }
  return null;
}

export function hoveredView() {
  for (const view of views.values()) if (view.pointer) return view;
  return null;
}

function markAll() {
  for (const view of views.values()) markPane(view.pane.id);
}

function uploadPump(id) {
  const socket = state.socket;
  if (socket && socket.bufferedAmount > 2 * 1024 * 1024) {
    setTimeout(() => uploadPump(id), 10);
    return;
  }
  const more = state.client.upload_next(id);
  flush();
  if (more) setTimeout(() => uploadPump(id), 10);
}

export async function uploadImage(pane, file) {
  const extension = { "image/png": "png", "image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp" }[file.type] || "";
  let id;
  try {
    id = state.client.upload_image(pane, extension, new Uint8Array(await file.arrayBuffer()));
  } catch (error) {
    pasteError(error.message || String(error));
    return;
  }
  flush();
  uploadPump(id);
}

paneRenderer("terminal", terminal);
on("appearance", measure);
on("theme", markAll);
on("focus", markAll);
on("commandOutput", markAll);
on("disconnected", () => state.client?.upload_cancel());
on("clipboard", (event) => {
  navigator.clipboard?.writeText(event.text).catch(() => {});
});
on("openUri", (event) => {
  if (state.client.openable_uri(event.uri)) open(event.uri, "_blank", "noopener,noreferrer");
  else pasteError(`Cannot open this URI: ${event.uri}`);
});
on("terminalUiCommand", (event) => {
  const view = views.get(event.pane);
  if (!view) return;
  view.searchShown = false;
  markPane(event.pane);
});
on("history", (event) => {
  if (!event.resume) return;
  const resume = () => {
    const wait = state.client.history_resume(event.pane);
    flush();
    if (wait) setTimeout(resume, BACKFILL_QUIET_MS);
  };
  setTimeout(resume, BACKFILL_QUIET_MS);
});
for (const type of ["kittyChunk", "kittyRemoved"]) {
  on(type, (event) => {
    if (type === "kittyRemoved") views.get(event.pane)?.forgetImages(event.images);
    if (type === "kittyChunk" && event.complete) views.get(event.pane)?.forgetImages([event.image]);
    markPane(event.pane);
  });
}
on("message", (event) => {
  if (event.name.startsWith("PastedImage")) for (const view of views.values()) if (view.hover.image != null) markPane(view.pane.id);
});
document.addEventListener("focusin", markAll);
document.addEventListener("focusout", markAll);
for (const type of ["keydown", "keyup"]) {
  document.addEventListener(type, (event) => {
    if (!["Shift", "Control", "Alt", "Meta"].includes(event.key)) return;
    const view = hoveredView();
    if (!view?.pointer || view.dragging) return;
    view.sync();
    state.client.mouse_modifiers(view.pane.id, view.output, ...view.pointer, ...mods(event));
    flush();
  });
}
document.fonts?.addEventListener?.("loadingdone", () => {
  if (state.client && state.appearance) measure();
});
