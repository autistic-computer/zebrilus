import { emit, flush, markTree, on, phase, schedule, state } from "./core.js";
import { floatRows, paneHeaderButton, paneIcon, paneRenderer } from "./workspace.js";

const TOOL_ICONS = {
  read: "file",
  search: "search",
  edit: "pencil",
  execute: "square-terminal",
  fetch: "globe",
  think: "cpu",
  other: "asterisk",
};
const COUNT_ICONS = ["file", "search", "pencil", "square-terminal", "globe", "asterisk", "bot", "triangle-alert"];
const JUMP_DISTANCE = 320;
const STICK_DISTANCE = 70;

const lenses = new Map();
const dirty = new Set();

function h(tag, props = {}, ...children) {
  const element = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === "class") element.className = value;
    else if (key === "text") element.textContent = value;
    else if (key.startsWith("on")) element.addEventListener(key.slice(2), value);
    else if (key === "dataset") Object.assign(element.dataset, value);
    else element.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child === null || child === undefined || child === false) continue;
    element.append(child instanceof Node ? child : document.createTextNode(String(child)));
  }
  return element;
}

const icon = (name, extra = {}) => h("i", { "data-icon": name, ...extra });
const text = (value, props = {}) => h("span", { "data-text": true, ...props, text: value });

function button(zz, title, iconName, onclick, props = {}) {
  return h("button", { "data-penna": zz, title, onclick, ...props }, iconName ? icon(iconName) : null);
}

function copy(value) {
  navigator.clipboard?.writeText(value).catch(() => {});
}

function spinner() {
  return icon("loader", { class: "agent-spin" });
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[char]);
}

const colorCanvas = document.createElement("canvas");
colorCanvas.width = 1;
colorCanvas.height = 1;
const colorCtx = colorCanvas.getContext("2d", { willReadFrequently: true });

function cssToHex(value, fallback) {
  if (!value || !colorCtx) return fallback;
  colorCtx.clearRect(0, 0, 1, 1);
  colorCtx.fillStyle = "#000000";
  colorCtx.fillStyle = value;
  colorCtx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = colorCtx.getImageData(0, 0, 1, 1).data;
  if (a === 0) return fallback;
  return `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

function mermaidTheme() {
  const styles = getComputedStyle(document.documentElement);
  const read = (name, fallback) => cssToHex(styles.getPropertyValue(name).trim(), fallback);
  const accent = read("--penna-accent", "#5b8def");
  const success = read("--penna-success", "#4ea874");
  const warning = read("--penna-warning", "#d9a441");
  const danger = read("--penna-danger", "#d96060");
  const bg = read("--penna-bg", "#1a1a1c");
  const fg = read("--penna-fg", "#e6e6e8");
  const muted = read("--penna-muted", "#9a9aa0");
  const panel = read("--penna-panel", "#232326");
  const divider = read("--penna-divider", "#36363c");
  const dark = document.documentElement.dataset.pennaMode !== "light";
  const node = cssToHex(dark ? `color-mix(in oklab, ${panel} 82%, ${accent} 18%)` : `color-mix(in oklab, ${panel} 88%, ${accent} 12%)`, panel);
  const nodeBorder = cssToHex(dark ? `color-mix(in oklab, ${divider} 55%, ${accent} 45%)` : `color-mix(in oklab, ${divider} 60%, ${accent} 40%)`, divider);
  return {
    background: bg,
    foreground: fg,
    muted,
    surface: panel,
    surfaceAlt: bg,
    border: divider,
    node,
    nodeBorder,
    accent,
    success,
    warning,
    danger,
    series: [accent, success, warning, danger],
    font: styles.getPropertyValue("--penna-font").trim() || "Plumula Sans, system-ui, sans-serif",
    mono: styles.getPropertyValue("--penna-mono").trim() || "Lilex, ui-monospace, monospace",
    radius: parseFloat(styles.getPropertyValue("--penna-radius")) || 8,
  };
}

export function renderMermaid(source) {
  if (!state.client) return { ok: false };
  try {
    return JSON.parse(state.client.render_mermaid(source, JSON.stringify(mermaidTheme())));
  } catch (error) {
    return { ok: false, error: String(error) };
  }
}

const KEYWORDS = new Set([
  "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "crate", "def", "default",
  "defer", "do", "dyn", "elif", "else", "enum", "export", "extends", "extern", "false", "finally", "fn", "for",
  "from", "func", "function", "go", "if", "impl", "import", "in", "interface", "is", "let", "loop", "match",
  "mod", "move", "mut", "new", "nil", "none", "null", "package", "pass", "pub", "raise", "ref", "return",
  "self", "Self", "static", "struct", "super", "switch", "then", "this", "throw", "trait", "true", "try",
  "type", "typeof", "undefined", "unsafe", "use", "var", "void", "where", "while", "with", "yield",
]);

export function highlightCode(source, language = "text") {
  const clean = source.replace(/\n$/, "");
  const lang = (language || "text").toLowerCase();
  if (lang === "text" || lang === "plain" || clean.length > 32768) return escapeHtml(clean);
  const pattern = /(\/\/[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b\d+(?:\.\d+)?\b|\b[A-Z][A-Za-z0-9_]*\b|\b[a-zA-Z_][A-Za-z0-9_]*\b)/g;
  let out = "";
  let last = 0;
  for (const match of clean.matchAll(pattern)) {
    const start = match.index;
    const token = match[0];
    if (start > last) out += escapeHtml(clean.slice(last, start));
    last = start + token.length;
    let cls = "";
    if (token.startsWith("//") || token.startsWith("/*") || (token.startsWith("#") && !["css", "html", "xml"].includes(lang))) {
      cls = "hl-cmt";
    } else if (token[0] === '"' || token[0] === "'" || token[0] === "`") {
      cls = "hl-str";
    } else if (/^\d/.test(token)) {
      cls = "hl-num";
    } else if (KEYWORDS.has(token)) {
      cls = "hl-kw";
    } else if (/^[A-Z]/.test(token)) {
      cls = "hl-type";
    } else if (clean[last] === "(") {
      cls = "hl-fn";
    }
    out += cls ? `<span class="${cls}">${escapeHtml(token)}</span>` : escapeHtml(token);
  }
  if (last < clean.length) out += escapeHtml(clean.slice(last));
  return out;
}

function figureCard(paneId, title, language, source, svgHtml) {
  const openPip = () => {
    if (paneId == null || !state.client) return;
    state.client.open_view_pane(paneId, title, "diagram", language, source, floatRows());
    flush();
  };
  const canvas = h("div", { class: "agent-figure-canvas" });
  canvas.innerHTML = svgHtml;
  const card = h(
    "div",
    {
      class: "agent-figure",
      "data-penna": "card.activity",
      role: "button",
      title: "Open in floating view pane",
      onclick: openPip,
    },
    canvas,
    h(
      "div",
      { class: "agent-figure-bar" },
      text(title, { class: "agent-figure-caption" }),
      h(
        "div",
        { class: "agent-figure-actions" },
        button("icon-button.row", "Copy source", "copy", (event) => {
          event.stopPropagation();
          copy(source);
        }),
        button("icon-button.row", "Open in floating view", "picture-in-picture", (event) => {
          event.stopPropagation();
          openPip();
        }),
      ),
    ),
  );
  return card;
}

function htmlVisualCard(source) {
  const styles = getComputedStyle(document.documentElement);
  const vars = ["--penna-bg", "--penna-fg", "--penna-muted", "--penna-panel", "--penna-divider", "--penna-accent", "--penna-success", "--penna-warning", "--penna-danger", "--penna-font", "--penna-mono"]
    .map((name) => `${name}:${styles.getPropertyValue(name).trim() || "inherit"};`)
    .join("");
  const doc = `<!doctype html><html><head><meta charset="utf-8"><style>:root{${vars}color-scheme:${document.documentElement.dataset.pennaMode === "light" ? "light" : "dark"};}html,body{margin:0;padding:12px;background:transparent;color:var(--penna-fg);font:13px/1.5 var(--penna-font);}</style></head><body>${source}</body></html>`;
  let showingCode = false;
  const iframe = h("iframe", { class: "agent-visual-frame", sandbox: "allow-scripts", srcdoc: doc });
  const pre = h("pre", { class: "agent-visual-code", hidden: true });
  pre.innerHTML = highlightCode(source, "html");
  const toggleBtn = button("icon-button.row", "Toggle source", "braces", () => {
    showingCode = !showingCode;
    iframe.hidden = showingCode;
    pre.hidden = !showingCode;
  });
  return h(
    "div",
    { class: "agent-visual", "data-penna": "card.activity" },
    iframe,
    pre,
    h(
      "div",
      { class: "agent-figure-bar" },
      text("HTML", { class: "agent-figure-caption" }),
      h(
        "div",
        { class: "agent-figure-actions" },
        toggleBtn,
        button("icon-button.row", "Copy HTML", "copy", () => copy(source)),
      ),
    ),
  );
}

function markdown(html, paneId = null) {
  const root = h("div", { class: "agent-md" });
  root.innerHTML = html || "";
  for (const pre of [...root.querySelectorAll("pre")]) {
    const code = pre.querySelector("code");
    const language = (code?.className.match(/language-(\S+)/)?.[1] || "text").toLowerCase();
    const source = code?.textContent ?? pre.textContent;
    if (language === "mermaid") {
      const rendered = renderMermaid(source);
      if (rendered?.ok && rendered.svg) {
        pre.replaceWith(figureCard(paneId, rendered.title || "Diagram", "mermaid", source, rendered.svg));
        continue;
      }
    }
    if (language === "svg" && source.includes("<svg")) {
      pre.replaceWith(figureCard(paneId, "SVG", "svg", source, source));
      continue;
    }
    if (language === "html" && /<(?:html|body|div|svg|canvas|style|section|table)\b/i.test(source)) {
      pre.replaceWith(htmlVisualCard(source));
      continue;
    }
    const body = h("pre");
    body.innerHTML = highlightCode(source, language);
    const block = h(
      "div",
      { class: "agent-code", "data-penna": "card.code" },
      h(
        "div",
        { class: "agent-code-chrome" },
        text(language, { class: "agent-code-language" }),
        h(
          "div",
          { class: "agent-code-actions" },
          paneId != null
            ? button("icon-button.row", "Open in floating view", "picture-in-picture", () => {
                state.client?.open_view_pane(paneId, language, "code", language, source, floatRows());
                flush();
              }, { class: "agent-code-copy" })
            : null,
          button("icon-button.row", "Copy code", "copy", () => copy(source), { class: "agent-code-copy" }),
        ),
      ),
      body,
    );
    pre.replaceWith(block);
  }
  const depthOf = (list) => {
    let depth = 0;
    for (let at = list.parentElement; at && at !== root; at = at.parentElement) if (at.matches("ul, ol")) depth++;
    return depth;
  };
  for (const list of [...root.querySelectorAll("ul, ol")]) {
    const ordered = list.tagName === "OL";
    const depth = depthOf(list);
    const start = Number(list.getAttribute("start") || 1);
    [...list.children].forEach((item, index) => {
      const checkbox = item.querySelector(":scope > input[type=checkbox], :scope > p > input[type=checkbox]");
      let prefix;
      if (checkbox) {
        prefix = h("span", { class: "agent-md-check" }, h("span", { class: "agent-md-box", "data-checked": checkbox.checked || null }, checkbox.checked ? icon("check") : null));
        checkbox.remove();
      } else if (ordered) {
        const n = start + index;
        const label =
          depth === 0 ? `${n}. ` : depth === 1 ? `${String.fromCharCode(65 + ((n - 1) % 26))}. ` : `${String.fromCharCode(97 + ((n - 1) % 26))}. `;
        prefix = text(label, { class: "agent-md-bullet" });
      } else {
        prefix = text(`${["•", "◦", "▪", "‣", "⁃"][Math.min(depth, 4)]} `, { class: "agent-md-bullet" });
      }
      const body = h("div", { class: "agent-md-item" });
      body.append(...item.childNodes);
      item.replaceChildren(prefix, body);
    });
  }
  for (const code of root.querySelectorAll(":not(pre) > code")) code.dataset.penna = "card.code";
  for (const link of root.querySelectorAll("a")) {
    link.target = "_blank";
    link.rel = "noreferrer";
  }
  return root;
}

function activity({ glyph, tint, label, labelTone, detail, counts = [], tag, end, onclick, title }) {
  const row = h("div", { class: "agent-activity", "data-clickable": end ? true : null, title, onclick });
  if (glyph) row.append(h("span", { class: "agent-glyph", "data-tint": tint }, glyph));
  row.append(text(label, { class: "agent-activity-label", "data-tone": labelTone }));
  if (detail) row.append(text(detail, { class: "agent-activity-detail" }));
  if (counts.length) {
    row.append(
      h(
        "span",
        { class: "agent-counts" },
        counts.map((count) =>
          h(
            "span",
            { class: "agent-count", title: count.describe, "data-failed": count.failed || null },
            icon(COUNT_ICONS[count.slot]),
            h("span", { class: "agent-count-badge", "data-penna": "badge", "data-tone": count.failed ? "danger" : null, "data-prominent": count.failed || null }, text(String(count.count))),
          ),
        ),
      ),
    );
  }
  if (tag) row.append(h("span", { class: "agent-tag", "data-penna": "badge", "data-tone": tag.danger ? "danger" : null }, text(tag.text)));
  if (end) row.append(icon(end, { class: "agent-chevron" }));
  return row;
}

function toolRow(lens, tool, keyMode = false) {
  const glyph = tool.spin ? spinner() : icon(tool.agent ? "bot" : TOOL_ICONS[tool.tool] || "asterisk");
  return activity({
    glyph,
    tint: tool.tint,
    label: tool.label,
    tag: tool.tag,
    end: tool.opens ? "external-link" : null,
    title: tool.opens ? "Open the output in a pane" : null,
    onclick: tool.opens
      ? () =>
          act(lens, () =>
            keyMode
              ? state.client.agent_open_output_key(lens.pane, String(tool.id))
              : state.client.agent_open_output(lens.pane, tool.id),
          )
      : null,
  });
}

function renderChatUnit(lens, unit) {
  if (!unit) return null;
  if (unit.kind === "text") {
    const block = h("div", { class: "agent-answer" }, markdown(unit.html, lens.pane));
    if (unit.copy !== null && unit.copy !== undefined) {
      block.append(h("div", { class: "agent-answer-actions" }, button("icon-button.row", "Copy message", "copy", () => copy(unit.copy))));
    }
    return block;
  }
  if (unit.kind === "notice") {
    return h("div", { class: "agent-notice" }, h("span", { class: "agent-notice-icon" }, icon("info")), h("div", { class: "agent-notice-text" }, markdown(unit.html, lens.pane)));
  }
  if (unit.kind === "run") {
    const box = h("div", { class: "agent-trace" });
    box.append(
      activity({
        glyph: unit.running ? spinner() : null,
        label: unit.label,
        labelTone: unit.running ? "foreground" : null,
        detail: unit.detail,
        end: unit.expanded ? "chevron-down" : "chevron-right",
        onclick: () => act(lens, () => state.client.agent_toggle_key(lens.pane, unit.key)),
      }),
    );
    if (unit.expanded && unit.steps?.length) {
      const steps = h("div", { class: "agent-steps", "data-penna": "rule.tree" });
      for (const step of unit.steps) {
        if (step.kind === "thought") {
          steps.append(
            h(
              "div",
              { class: "agent-step-thought" },
              activity({ glyph: icon("cpu"), label: "Thought" }),
              step.html ? h("div", { class: "agent-step-text" }, markdown(step.html, lens.pane)) : null,
            ),
          );
        } else {
          steps.append(toolRow(lens, step, true));
        }
      }
      box.append(steps);
    }
    return box;
  }
  if (unit.kind === "subagent") {
    const box = h("div", { class: "agent-trace" });
    const working = unit.status === "working";
    box.append(
      activity({
        glyph: working ? spinner() : icon("bot"),
        tint: unit.status === "failed" ? "danger" : unit.status === "waiting" ? "warning" : null,
        label: unit.title,
        detail: [unit.summary, unit.elapsed].filter(Boolean).join(" · ") || null,
        end: unit.expanded ? "chevron-down" : "chevron-right",
        onclick: () => act(lens, () => state.client.agent_toggle_key(lens.pane, unit.key)),
      }),
    );
    if (unit.expanded && unit.children?.length) {
      const steps = h("div", { class: "agent-steps", "data-penna": "rule.tree" });
      for (const child of unit.children) {
        const node = renderChatUnit(lens, child);
        if (node) steps.append(node);
      }
      box.append(steps);
    }
    return box;
  }
  if (unit.kind === "agents") {
    return h("div", { class: "agent-trace" }, ...(unit.agents || []).map((child) => renderChatUnit(lens, child)));
  }
  if (unit.kind === "plan" && unit.items?.length) {
    return h(
      "div",
      { class: "agent-plan", "data-penna": "inline-panel" },
      unit.items.map((item) =>
        h(
          "div",
          { class: "agent-plan-item", "data-state": item.state },
          h("span", { class: "agent-plan-mark" }, item.state === "inProgress" ? h("span", { class: "agent-plan-dot" }) : h("span", { class: "agent-md-box", "data-checked": item.state === "done" || null }, item.state === "done" ? icon("check") : null)),
          h("div", { class: "agent-plan-text" }, item.text),
        ),
      ),
    );
  }
  if (unit.kind === "record" && unit.answers?.length) {
    return h(
      "div",
      { class: "agent-record", "data-penna": "inline-panel" },
      unit.answers.map((pair) => h("div", { class: "agent-record-row" }, text(pair.question, { class: "agent-record-q" }), text(pair.answer, { class: "agent-record-a" }))),
    );
  }
  return null;
}

function renderChatTurn(lens, row) {
  const turn = h("div", { class: "agent-turn" });
  if (row.fold) {
    const fold = row.fold;
    const box = h("div", { class: "agent-trace" });
    box.append(
      activity({
        glyph: fold.state === "live" ? spinner() : null,
        tint: fold.state === "failed" ? "danger" : null,
        label: fold.label,
        detail: fold.activity || null,
        end: fold.expanded ? "chevron-down" : "chevron-right",
        onclick: () => act(lens, () => state.client.agent_toggle_key(lens.pane, fold.key)),
      }),
    );
    if (fold.expanded && fold.units?.length) {
      const steps = h("div", { class: "agent-steps", "data-penna": "rule.tree" });
      for (const unit of fold.units) {
        const node = renderChatUnit(lens, unit);
        if (node) steps.append(node);
      }
      box.append(steps);
    }
    turn.append(box);
  }
  for (const unit of row.units || []) {
    const node = renderChatUnit(lens, unit);
    if (node) turn.append(node);
  }
  return turn;
}

function trace(lens, turn) {
  const trace = turn.trace;
  const box = h("div", { class: "agent-trace" });
  const waiting = trace.state === "waiting";
  box.append(
    activity({
      glyph: trace.state === "live" ? spinner() : waiting ? icon("triangle-alert") : null,
      tint: waiting ? "warning" : null,
      label: trace.label,
      labelTone: waiting ? "warning" : trace.tool ? "foreground" : null,
      detail: trace.detail,
      counts: trace.counts,
      end: trace.expanded ? "chevron-up" : "chevron-right",
      onclick: () => act(lens, () => state.client.agent_toggle(lens.pane, turn.id)),
    }),
  );
  if (trace.thought) box.append(h("div", { class: "agent-thought" }, h("div", { class: "agent-thought-text", "aria-label": trace.thought }, trace.thought)));
  if (trace.expanded) {
    const steps = h("div", { class: "agent-steps", "data-penna": "rule.tree" });
    for (const step of trace.steps) {
      if (!step) continue;
      if (step.kind === "tool") steps.append(toolRow(lens, step));
      else if (step.kind === "thought") {
        steps.append(
          h(
            "div",
            { class: "agent-step-thought" },
            activity({ glyph: icon("cpu"), label: "Thought" }),
            step.html ? h("div", { class: "agent-step-text" }, markdown(step.html)) : null,
          ),
        );
      } else steps.append(h("div", { class: "agent-step-note" }, markdown(step.html)));
    }
    box.append(steps);
  }
  if (trace.failure) {
    box.append(
      h(
        "div",
        { class: "agent-failure" },
        toolRow(lens, trace.failure.tool),
        trace.failure.tail.map((line) => h("div", { class: "agent-failure-line" }, line)),
      ),
    );
  }
  return box;
}

function userBubble(lens, user) {
  const bubble = h(
    "div",
    { class: "agent-bubble", "data-penna": "card.bubble" },
    user.images.length ? h("div", { class: "agent-attachments" }, user.images.map((src) => h("img", { class: "agent-attachment", "data-penna": "card.image", src, alt: "" }))) : null,
    user.html ? markdown(user.html) : null,
  );
  const entry = h("div", { class: "agent-user" });
  if (!user.rewind) {
    entry.append(bubble);
    return entry;
  }
  entry.append(
    h(
      "div",
      { class: "agent-user-line" },
      h(
        "span",
        { class: "agent-rewind", "data-enabled": user.rewind.enabled || null },
        button(
          "icon-button.row",
          "Rewind to here: continue from before this prompt. Files stay as they are.",
          "history",
          () => act(lens, () => state.client.agent_rewind(lens.pane, user.rewind.id)),
        ),
      ),
      bubble,
    ),
  );
  return entry;
}

function renderRow(lens, row) {
  if (row.kind === "user") return userBubble(lens, row);
  if (row.kind === "chatTurn") return renderChatTurn(lens, row);
  if (row.kind === "notice") {
    return h("div", { class: "agent-notice" }, h("span", { class: "agent-notice-icon" }, icon("info")), h("div", { class: "agent-notice-text" }, markdown(row.html, lens.pane)));
  }
  if (row.kind === "reply") {
    return h(
      "div",
      { class: "agent-reply" },
      userBubble(lens, row.user),
      row.replies.map((reply) =>
        h(
          "div",
          { class: "agent-reply-popover", "data-penna": "inline-panel" },
          reply.side ? h("div", { class: "agent-reply-side" }, "Side answer · not in the conversation") : null,
          markdown(reply.html, lens.pane),
        ),
      ),
    );
  }
  const turn = h("div", { class: "agent-turn" });
  if (row.trace) turn.append(trace(lens, row));
  for (const answer of row.answers) {
    const block = h("div", { class: "agent-answer" }, markdown(answer.html, lens.pane));
    if (answer.copy !== null && answer.copy !== undefined) {
      block.append(h("div", { class: "agent-answer-actions" }, button("icon-button.row", "Copy message", "copy", () => copy(answer.copy))));
    }
    turn.append(block);
  }
  return turn;
}

function startsTurn(row) {
  return row.kind === "user" || row.kind === "reply";
}

function renderRows(lens, rows) {
  const seen = new Set();
  const order = [];
  rows.forEach((row, index) => {
    if (!row) return;
    const json = JSON.stringify(row);
    let cached = lens.rowCache.get(row.key);
    if (!cached || cached.json !== json) {
      const element = h(
        "div",
        { class: "agent-row", "data-key": row.key, "data-gap": index > 0 && startsTurn(row) ? true : null },
        h("div", { class: "agent-content" }, renderRow(lens, row)),
      );
      cached?.element.replaceWith(element);
      cached = { json, element };
      lens.rowCache.set(row.key, cached);
    } else {
      cached.element.toggleAttribute("data-gap", index > 0 && startsTurn(row));
    }
    seen.add(row.key);
    order.push(cached.element);
  });
  for (const [key, cached] of lens.rowCache) {
    if (!seen.has(key)) {
      cached.element.remove();
      lens.rowCache.delete(key);
    }
  }
  let previous = null;
  for (const element of order) {
    const expected = previous ? previous.nextSibling : lens.list.firstChild;
    if (expected !== element) lens.list.insertBefore(element, expected);
    previous = element;
  }
}

function empty(view) {
  if (view.empty?.welcome) {
    const welcome = view.empty.welcome;
    return h(
      "div",
      { class: "agent-welcome" },
      icon("bot", { class: "agent-welcome-icon" }),
      text(welcome.title, { class: "agent-welcome-title" }),
      text(welcome.detail, { class: "agent-welcome-detail" }),
    );
  }
  return h("div", { class: "agent-empty" }, view.empty.busy ? spinner() : null, text(view.empty.text));
}

function askQuestion(lens, action) {
  act(lens, () => state.client.agent_question(lens.pane, JSON.stringify(action)));
}

function askSend(lens, ask) {
  if (ask.kind === "question") {
    askQuestion(lens, { enter: true });
    return;
  }
  const sent = state.client.agent_approve(lens.pane, lens.reply.value);
  flush();
  if (sent) {
    lens.reply.value = "";
    grow(lens, lens.reply);
    lens.pinned = true;
  }
  mark(lens.pane);
}

function askPick(lens, ask, index, option) {
  if (ask.kind === "question") askQuestion(lens, { pick: [ask.index, index] });
  else act(lens, () => state.client.agent_respond(lens.pane, ask.requestId, option.id));
}

function askBody(lens, ask) {
  const disabled = !ask.writable || null;
  const header =
    ask.kind === "question"
      ? h("div", { class: "agent-ask-header", "data-question": true }, h("span", { class: "agent-ask-glyph" }, icon("chat")), text(ask.question, { class: "agent-ask-question" }))
      : h("div", { class: "agent-ask-header" }, icon(ask.icon, { class: "agent-ask-glyph" }), text(ask.title, { class: "agent-ask-title" }));
  if (ask.pager) {
    header.append(
      button("icon-button.row", "Previous question", "chevron-left", () => askQuestion(lens, { key: "left" }), { class: "agent-ask-step", disabled }),
      text(`${ask.pager.index + 1} of ${ask.pager.count}`, { class: "agent-ask-count" }),
      button("icon-button.row", "Next question", "chevron-right", () => askQuestion(lens, { key: "right" }), { class: "agent-ask-step", disabled }),
    );
  } else if (ask.counter) {
    header.append(text(ask.counter, { class: "agent-ask-count" }));
  }
  if (ask.kind === "plan") {
    header.append(button("icon-button.row", "Open the plan in a view pane", "expand", () => act(lens, () => state.client.agent_open_plan(lens.pane, ask.requestId)), { class: "agent-ask-step" }));
  }
  const parts = [header];
  if (ask.kind === "plan") {
    parts.push(h("div", { class: "agent-ask-sheet" }, markdown(ask.plan)));
  } else if (ask.inset || ask.note) {
    parts.push(
      h(
        "div",
        { class: "agent-ask-body" },
        ask.inset ? h("div", { class: "agent-ask-inset" }, ask.inset) : null,
        ask.note ? text(ask.note, { class: "agent-ask-note" }) : null,
      ),
    );
  }
  if (ask.options.length) {
    parts.push(
      h(
        "div",
        { class: "agent-ask-options" },
        ask.options.flatMap((option, index) => [
          option.divider ? h("div", { class: "agent-ask-divider", "data-penna": "rule.list" }) : null,
          h(
            "div",
            {
              class: "agent-ask-option",
              "data-penna": "menu-item",
              role: option.picked === null || option.picked === undefined ? "button" : "checkbox",
              "aria-checked": option.picked === null || option.picked === undefined ? null : String(option.picked),
              "data-state": !ask.writable ? "disabled" : option.highlighted ? "selected" : null,
              onclick: ask.writable ? () => askPick(lens, ask, index, option) : null,
            },
            h(
              "div",
              { class: "agent-ask-option-text", "data-under": option.under || null },
              text(option.label, { class: "agent-ask-option-label" }),
              option.detail ? text(option.detail, { class: "agent-ask-option-detail" }) : null,
            ),
            option.key || option.picked ? keycap(option.key, option.picked, !ask.writable) : null,
          ),
        ]),
      ),
    );
  }
  if (ask.problem) parts.push(text(ask.problem, { class: "agent-ask-problem" }));
  return parts;
}

function keycap(key, lit, disabled) {
  const look = lit ? { "data-emphasis": "solid", "data-solid": "accent" } : {};
  return h("span", { "data-penna": "keycap", "data-state": disabled ? "disabled" : null, ...look }, key ? text(key) : icon("check"));
}

function askLeading(ask) {
  if (!ask?.reply || !ask.leading) return [];
  return [keycap(ask.leading.key, ask.leading.lit, false)];
}

function askTrailing(lens, ask) {
  if (!ask) return [];
  const disabled = !ask.writable || null;
  return [
    ask.skip ? h("button", { "data-penna": "button.quiet", class: "agent-ask-skip", disabled, onclick: () => askQuestion(lens, { dismiss: true }) }, "Skip") : null,
    h(
      "button",
      { "data-penna": "button", "data-prominent": true, "data-tone": "primary", class: "agent-ask-send", disabled, onclick: () => askSend(lens, ask) },
      text(ask.send),
      icon("key-return"),
    ),
  ].filter(Boolean);
}

function tray(lens, tray) {
  const box = h("div", { class: "agent-tray" });
  if (tray.plan?.open) {
    box.append(
      h(
        "div",
        { class: "agent-tray-panel", "data-penna": "inline-panel" },
        h(
          "div",
          { class: "agent-plan" },
          tray.plan.items.map((item) =>
            h(
              "div",
              { class: "agent-plan-item", "data-state": item.state },
              h("span", { class: "agent-plan-mark" }, item.state === "inProgress" ? h("span", { class: "agent-plan-dot" }) : h("span", { class: "agent-md-box", "data-checked": item.state === "done" || null }, item.state === "done" ? icon("check") : null)),
              h("div", { class: "agent-plan-text" }, item.text),
            ),
          ),
        ),
      ),
    );
  }
  const bar = h("div", { class: "agent-tray-bar" });
  const chip = h("div", { class: "agent-tray-left" });
  if (tray.plan) {
    chip.append(
      h(
        "div",
        { class: "agent-plan-chip", "data-penna": "tab.tray", "data-state": tray.plan.open ? "current" : null, role: "button", onclick: () => act(lens, () => state.client.agent_plan(lens.pane)) },
        icon("circle-check"),
        text(tray.plan.label),
        tray.plan.current ? text(tray.plan.current, { class: "agent-plan-current" }) : null,
        icon(tray.plan.open ? "chevron-down" : "chevron-up", { class: "agent-plan-chevron" }),
      ),
    );
  }
  bar.append(chip);
  box.append(bar);
  return box;
}

function errorCard(error) {
  return h(
    "div",
    { class: "agent-alert", "data-penna": "inline-panel" },
    h("span", { class: "agent-alert-icon" }, icon("triangle-alert")),
    h("div", { class: "agent-alert-copy" }, error.stopped ? h("div", { class: "agent-alert-title" }, "The agent stopped") : null, h("div", { class: "agent-alert-text", "data-titled": error.stopped || null }, error.text)),
  );
}

function completions(lens, view) {
  const list = h("div", { class: "agent-completions", "data-penna": "inline-panel" });
  view.completions.forEach((completion, index) => {
    const row = h(
      "div",
      {
        class: "agent-completion",
        "data-penna": "menu-item",
        "data-state": index === view.completionSelected ? "selected" : null,
        onmouseenter: () => {
          if (index !== lens.view?.completionSelected) act(lens, () => state.client.agent_completion_select(lens.pane, index));
        },
        onmousedown: (event) => event.preventDefault(),
        onclick: () => accept(lens, index),
      },
      h("div", { class: "agent-completion-copy" }, h("div", { class: "agent-completion-name" }, `/${completion.name}`), completion.description ? h("div", { class: "agent-completion-detail" }, completion.description) : null),
    );
    list.append(row);
  });
  requestAnimationFrame(() => list.querySelector("[data-state~=selected]")?.scrollIntoView({ block: "nearest" }));
  return list;
}

function usageMeter(usage) {
  const radius = 5.5;
  const length = 2 * Math.PI * radius;
  const fraction = usage ? Math.min(1, Math.max(0, usage.fraction || 0)) : 0;
  const label = usage?.percent || "--";
  const tooltip = usage?.tooltip || "Context window usage";
  const ring = `<svg width="14" height="14" viewBox="0 0 14 14"><circle cx="7" cy="7" r="${radius}" fill="none" stroke-width="1.6" class="agent-usage-track"/><circle cx="7" cy="7" r="${radius}" fill="none" stroke-width="1.6" class="agent-usage-fill" stroke-dasharray="${(fraction * length).toFixed(2)} ${length.toFixed(2)}" transform="rotate(-90 7 7)"/></svg>`;
  const meter = h("div", { class: "agent-usage-pill", role: "progressbar", "aria-label": tooltip, title: tooltip });
  meter.innerHTML = ring;
  meter.append(text(label, { class: "agent-usage-label" }));
  return meter;
}

let modelPopover = null;

let modelPopoverCleanup = null;

function closeModelPopover() {
  modelPopoverCleanup?.();
  modelPopoverCleanup = null;
  modelPopover?.remove();
  modelPopover = null;
}

function openModelPopover(lens, settings, anchor) {
  if (modelPopover?.owner === anchor) {
    closeModelPopover();
    return;
  }
  closeModelPopover();
  const pick = (option, value) => {
    act(lens, () => state.client.agent_set_config(lens.pane, option, value));
    closeModelPopover();
  };
  const panel = h("div", { class: "penna-menu agent-model-popover", "data-penna": "popover" });
  panel.owner = anchor;
  if (settings.models?.length) {
    panel.append(h("div", { class: "agent-popover-head", text: "Model" }));
    for (const choice of settings.models) {
      const selected = choice.value === settings.model;
      panel.append(
        h(
          "div",
          {
            "data-penna": "menu-item",
            class: "agent-model-item",
            "data-state": selected ? "selected" : null,
            onclick: () => pick("model", choice.value),
          },
          text(choice.name || choice.value, { class: "penna-label" }),
          selected ? icon("check") : null,
        ),
      );
    }
  }
  if (settings.efforts?.length) {
    if (panel.childElementCount) panel.append(h("div", { class: "agent-ask-divider", "data-penna": "rule.list" }));
    panel.append(h("div", { class: "agent-popover-head", text: "Effort" }));
    const seg = h(
      "div",
      { class: "penna-segmented agent-effort-seg", "data-penna": "field", "data-size": "small", "data-width": "full" },
      h(
        "div",
        { class: "penna-segments" },
        settings.efforts.map((choice) =>
          h(
            "button",
            {
              class: "penna-segment",
              type: "button",
              "aria-selected": String(choice.value === settings.effort),
              onclick: () => pick("effort", choice.value),
            },
            text(choice.name || choice.value, { class: "penna-label" }),
          ),
        ),
      ),
    );
    panel.append(h("div", { class: "agent-popover-pad" }, seg));
  }
  if (settings.modes?.length) {
    if (panel.childElementCount) panel.append(h("div", { class: "agent-ask-divider", "data-penna": "rule.list" }));
    panel.append(h("div", { class: "agent-popover-head", text: "Permissions" }));
    for (const choice of settings.modes) {
      const selected = choice.value === settings.mode;
      panel.append(
        h(
          "div",
          {
            "data-penna": "menu-item",
            class: "agent-model-item",
            "data-state": selected ? "selected" : null,
            onclick: () => pick("mode", choice.value),
          },
          text(choice.name || choice.value, { class: "penna-label" }),
          selected ? icon("check") : null,
        ),
      );
    }
  }
  document.body.append(panel);
  const rect = anchor.getBoundingClientRect();
  const bounds = panel.getBoundingClientRect();
  const left = Math.max(8, Math.min(rect.right - bounds.width, innerWidth - bounds.width - 8));
  const top = Math.max(8, rect.top - bounds.height - 6);
  panel.style.left = `${left}px`;
  panel.style.top = `${top}px`;
  modelPopover = panel;
  const dismiss = (event) => {
    if (panel.contains(event.target) || anchor.contains(event.target)) return;
    closeModelPopover();
  };
  const onEscape = (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    closeModelPopover();
  };
  modelPopoverCleanup = () => {
    removeEventListener("pointerdown", dismiss, true);
    removeEventListener("keydown", onEscape, true);
  };
  setTimeout(() => {
    if (modelPopover !== panel) return;
    addEventListener("pointerdown", dismiss, true);
    addEventListener("keydown", onEscape, true);
  });
}

function modelPickerButton(lens, settings, enabled) {
  const hasChoices = Boolean(settings.models?.length || settings.efforts?.length || settings.modes?.length);
  const label = settings.modelLabel || settings.model || "Model";
  return h(
    "button",
    {
      "data-penna": "button.quiet",
      "data-size": "small",
      class: "agent-model-trigger",
      disabled: !enabled || !hasChoices || null,
      title: "Choose the model and effort",
      onclick: (event) => {
        event.stopPropagation();
        openModelPopover(lens, settings, event.currentTarget);
      },
    },
    text(label, { class: "agent-model-name" }),
    settings.effortName ? text(settings.effortName, { class: "agent-model-effort" }) : null,
    icon("chevron-down", { class: "agent-model-chevron" }),
  );
}

function mentionQuery(input) {
  const cursor = input.selectionStart ?? input.value.length;
  const head = input.value.slice(0, cursor);
  const match = head.match(/(?:^|\s)@([^\s]*)$/);
  if (!match) return null;
  return { start: cursor - match[1].length - 1, end: cursor, query: match[1] };
}

function acceptMention(lens, index) {
  const mention = lens.mentions?.[index ?? lens.mentionSelected ?? 0];
  const query = mentionQuery(lens.input);
  if (!mention || !query) return false;
  const value = lens.input.value;
  const next = `${value.slice(0, query.start)}${mention.insert}${value.slice(query.end)}`;
  const at = query.start + mention.insert.length;
  lens.input.value = next;
  lens.input.focus();
  lens.input.setSelectionRange(at, at);
  lens.mentions = [];
  lens.mentionSelected = 0;
  grow(lens);
  mark(lens.pane);
  return true;
}

function mentionsList(lens) {
  const list = h("div", { class: "agent-completions", "data-penna": "inline-panel" });
  lens.mentions.forEach((item, index) => {
    list.append(
      h(
        "div",
        {
          class: "agent-completion",
          "data-penna": "menu-item",
          "data-state": index === lens.mentionSelected ? "selected" : null,
          onmouseenter: () => {
            if (lens.mentionSelected !== index) {
              lens.mentionSelected = index;
              mark(lens.pane);
            }
          },
          onmousedown: (event) => event.preventDefault(),
          onclick: () => acceptMention(lens, index),
        },
        icon(paneIcon(item.icon)),
        h(
          "div",
          { class: "agent-completion-copy" },
          h("div", { class: "agent-completion-name" }, item.label),
          item.detail ? h("div", { class: "agent-completion-detail" }, item.detail) : null,
        ),
      ),
    );
  });
  return list;
}

function act(lens, effect) {
  effect();
  flush();
  mark(lens.pane);
}

function accept(lens, index) {
  const result = state.client.agent_completion_accept(lens.pane, index ?? undefined);
  if (!result) return false;
  const { value, cursor } = JSON.parse(result);
  lens.input.value = value;
  lens.input.focus();
  lens.input.setSelectionRange(cursor, cursor);
  grow(lens);
  mark(lens.pane);
  return true;
}

function submit(lens) {
  const value = lens.input.value;
  const sent = state.client.agent_submit(lens.pane, value);
  flush();
  if (sent) {
    lens.input.value = "";
    grow(lens);
    lens.pinned = true;
    emit("agentSubmitted", { pane: lens.pane });
  }
  mark(lens.pane);
}

function grow(lens, input = lens.input) {
  input.style.height = "auto";
  const line = parseFloat(getComputedStyle(input).lineHeight) || 16;
  const padding = parseFloat(getComputedStyle(input).paddingTop) * 2 || 0;
  input.style.height = `${Math.min(input.scrollHeight, line * 8 + padding)}px`;
}

function keyName(event) {
  return { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Tab: "tab", Escape: "escape", Enter: "enter" }[event.key] || event.key;
}

function askKey(lens, field, event) {
  if (event.metaKey || event.altKey || event.ctrlKey) return false;
  const empty = !field.value.trim();
  const navigation = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Tab"].includes(event.key) && lens.view?.ask;
  if (!(["ArrowUp", "ArrowDown", "Escape"].includes(event.key) || navigation || (/^[1-9]$/.test(event.key) && empty))) return false;
  if (navigation && !empty) return false;
  if (!state.client.agent_key(lens.pane, keyName(event), empty, true)) return false;
  event.preventDefault();
  flush();
  mark(lens.pane);
  return true;
}

function createLens(pane) {
  const input = h("textarea", { class: "agent-input", rows: "1", spellcheck: "false" });
  const lens = {
    pane: pane.id,
    host: pane,
    input,
    rowCache: new Map(),
    pinned: true,
    view: null,
  };
  const list = h("div", { class: "agent-list" });
  const scroll = h("div", { class: "agent-scroll" }, list);
  const area = h("div", { class: "agent-area" }, scroll);
  const jump = button("icon-button.floating", "Jump to latest", "arrow-down", () => {
    lens.pinned = true;
    scroll.scrollTo({ top: scroll.scrollHeight, behavior: "smooth" });
  }, { class: "agent-jump" });
  area.append(jump);
  const prefix = h("div", { class: "agent-prefix" });
  const mention = button("button.send", "Mention a host, session or file", "plus", () => {
    const at = input.selectionStart;
    input.setRangeText("@", at, input.selectionEnd, "end");
    input.focus();
    mark(lens.pane);
  });
  const hint = h("div", { class: "agent-hint" });
  const reply = h("textarea", { class: "agent-input agent-reply", rows: "1", spellcheck: "false", hidden: true });
  const lead = h("div", { class: "agent-ask-leading", hidden: true });
  const ask = h("div", { class: "agent-ask", hidden: true });
  const askRule = h("div", { class: "agent-ask-rule", "data-penna": "rule.list", hidden: true });
  const trailing = h("div", { class: "agent-ask-trailing", hidden: true });
  const usage = h("div", { class: "agent-usage-slot" });
  const action = h("button", { "data-penna": "button.send", "data-prominent": true, "data-tone": "primary", class: "agent-action" }, icon("arrow-up"));
  action.addEventListener("click", () => {
    if (lens.view?.action === "stop") act(lens, () => state.client.agent_cancel(lens.pane));
    else submit(lens);
  });
  const row = h("div", { class: "agent-composer-row" }, mention, lead, h("div", { class: "agent-field" }, input, reply, hint), trailing, action);
  const pill = h("div", { class: "agent-pill", "data-penna": "field.composer" }, ask, askRule, row);
  const upper = h("div", { class: "agent-upper" }, h("div", { class: "agent-column agent-stack" }, prefix, h("div", { class: "agent-pill-back" }, pill)));
  const git = h("div", { class: "agent-git" });
  const meta = h("div", { class: "agent-footer-right" }, usage);
  const footerBar = h("div", { class: "agent-column agent-footer-bar" }, git, meta);
  const hints = h("div", { class: "agent-column agent-ask-hints", hidden: true });
  const composer = h("div", { class: "agent-composer" }, upper, h("div", { class: "agent-footer" }, footerBar, hints));
  const root = h("div", { class: "agent-chat" }, area, composer);
  Object.assign(lens, { root, list, scroll, area, jump, prefix, pill, upper, hint, usage, meta, footerBar, action, git, composer, mention, reply, lead, ask, askRule, trailing, hints, mentions: [], mentionSelected: 0 });

  scroll.addEventListener("scroll", () => {
    const distance = scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight;
    lens.pinned = distance <= STICK_DISTANCE;
    jump.hidden = !(lens.rowCache.size && distance > JUMP_DISTANCE);
  });
  input.addEventListener("input", () => {
    grow(lens);
    mark(lens.pane);
  });
  for (const field of [input, reply]) {
    field.addEventListener("focus", () => pill.setAttribute("data-state", "focused"));
    field.addEventListener("blur", () => pill.removeAttribute("data-state"));
  }
  reply.addEventListener("input", () => {
    grow(lens, reply);
    const current = lens.view?.ask;
    if (current?.kind === "question") state.client.agent_question(lens.pane, JSON.stringify({ otherText: [current.index, reply.value] }));
    mark(lens.pane);
  });
  reply.addEventListener("keydown", (event) => {
    if (event.isComposing) return;
    const current = lens.view?.ask;
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (current) askSend(lens, current);
      return;
    }
    askKey(lens, reply, event);
  });
  input.addEventListener("keyup", (event) => {
    if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) mark(lens.pane);
  });
  input.addEventListener("keydown", (event) => {
    if (event.isComposing) return;
    const view = lens.view;
    const hasCompletions = Boolean(view?.completions.length);
    const hasMentions = Boolean(lens.mentions?.length);
    if (hasMentions && !hasCompletions) {
      if (event.key === "ArrowUp") {
        event.preventDefault();
        lens.mentionSelected = (lens.mentionSelected - 1 + lens.mentions.length) % lens.mentions.length;
        mark(lens.pane);
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        lens.mentionSelected = (lens.mentionSelected + 1) % lens.mentions.length;
        mark(lens.pane);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        lens.mentions = [];
        mark(lens.pane);
        return;
      }
      if ((event.key === "Enter" || event.key === "Tab") && !event.shiftKey) {
        event.preventDefault();
        acceptMention(lens, lens.mentionSelected);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (hasCompletions && accept(lens, null)) return;
      submit(lens);
      return;
    }
    if (event.key === "Tab" && hasCompletions && !event.shiftKey) {
      event.preventDefault();
      accept(lens, null);
      return;
    }
    askKey(lens, input, event);
  });
  root.addEventListener("keydown", (event) => {
    if (event.target === input || event.target === reply || event.target.closest("input, textarea")) return;
    if (event.metaKey || event.altKey || event.ctrlKey) return;
    if (state.client.agent_key(lens.pane, keyName(event), !input.value.trim(), false)) {
      event.preventDefault();
      event.stopPropagation();
      flush();
      mark(lens.pane);
    }
  });
  root.addEventListener("mouseup", (event) => {
    event.stopPropagation();
    if (getSelection()?.toString() || event.target.closest("a, input, textarea")) return;
    (reply.hidden ? input : reply).focus({ preventScroll: true });
  });
  return lens;
}

function updateComposer(lens, view) {
  lens.input.placeholder = view.placeholder;
  lens.composer.hidden = false;
  lens.pill.parentElement.hidden = view.watchOnly;
  const prefix = [];
  updateAsk(lens, view);
  if (view.queued) {
    prefix.push(
      h(
        "div",
        { class: "agent-queue" },
        h(
          "button",
          {
            "data-penna": "button.quiet",
            "data-size": "small",
            class: "agent-queue-chip",
            "aria-label": `${view.queued} queued`,
            title: "Return the queued prompts to the composer",
            disabled: !view.writable || null,
            onclick: () => act(lens, () => state.client.agent_unqueue(lens.pane)),
          },
          icon("undo-2"),
          `${view.queued} queued`,
        ),
      ),
    );
  }
  if (view.tray) prefix.push(tray(lens, view.tray));
  if (view.error) prefix.push(errorCard(view.error));
  if (view.completions.length && !view.ask?.reply) {
    lens.mentions = [];
    prefix.push(completions(lens, view));
  } else if (!view.ask?.reply) {
    const mq = mentionQuery(lens.input);
    lens.mentions = mq && state.client ? JSON.parse(state.client.agent_mentions(lens.pane, mq.query)) : [];
    lens.mentionSelected = Math.min(lens.mentionSelected || 0, Math.max(0, lens.mentions.length - 1));
    if (lens.mentions.length) prefix.push(mentionsList(lens));
  } else {
    lens.mentions = [];
  }
  lens.prefix.replaceChildren(...prefix);
  lens.composer.toggleAttribute("data-prefix", prefix.length > 0);
  lens.hint.textContent = view.hint || "";
  lens.hint.hidden = !view.hint;
  const metaItems = [];
  if (!view.ask) {
    metaItems.push(usageMeter(view.usage));
    if (view.settings) metaItems.push(modelPickerButton(lens, view.settings, view.writable));
  }
  lens.meta.replaceChildren(...metaItems);
  const [iconName, title] = {
    send: ["arrow-up", "Send message"],
    queue: ["plus", "Queue this as the next turn"],
    stop: ["stop", "Stop the current turn"],
  }[view.action];
  lens.action.title = title;
  lens.action.firstChild.dataset.icon = iconName;
  lens.action.toggleAttribute("data-prominent", view.action !== "queue");
  if (view.action === "queue") delete lens.action.dataset.tone;
  else lens.action.dataset.tone = "primary";
  lens.action.disabled = !view.actionEnabled;
  lens.action.hidden = Boolean(view.ask) && view.action !== "stop";
  const git = view.git;
  lens.git.replaceChildren(
    ...(git
      ? [
          h(
            "div",
            { class: "agent-git-summary", title: git.tooltip },
            icon("git-branch"),
            git.branch ? text(git.branch, { class: "agent-git-branch" }) : null,
            text(git.files),
            text(git.additions, { class: "agent-git-add" }),
            text(git.deletions, { class: "agent-git-del" }),
          ),
        ]
      : []),
  );
  if (view.drafts.length) {
    lens.input.value = [lens.input.value, ...view.drafts].filter(Boolean).join("\n");
    grow(lens);
    mark(lens.pane);
  }
}

function updateAsk(lens, view) {
  const ask = view.ask;
  lens.pill.toggleAttribute("data-asking", Boolean(ask));
  const wasAsking = lens.pill.hasAttribute("data-asking");
  lens.pill.toggleAttribute("data-asking", Boolean(ask));
  if (wasAsking !== Boolean(ask)) globalThis.pennaKit?.roundAll?.();
  lens.ask.hidden = !ask;
  lens.askRule.hidden = !ask;
  lens.ask.replaceChildren(...(ask ? askBody(lens, ask) : []));
  const replying = Boolean(ask?.reply);
  const typing = document.activeElement === lens.input || document.activeElement === lens.reply;
  lens.input.hidden = replying;
  lens.reply.hidden = !replying;
  lens.mention.hidden = replying;
  if (replying) {
    if (lens.replyKey !== ask.reply.key) {
      lens.replyKey = ask.reply.key;
      lens.reply.value = ask.reply.value || "";
      grow(lens, lens.reply);
    }
    lens.reply.placeholder = ask.reply.placeholder;
    lens.reply.toggleAttribute("data-secret", ask.reply.secret);
  } else {
    lens.replyKey = null;
  }
  const row = replying ? lens.reply : lens.input;
  if (typing && document.activeElement !== row) row.focus({ preventScroll: true });
  const leading = askLeading(ask);
  lens.lead.replaceChildren(...leading);
  lens.lead.hidden = leading.length === 0;
  const trailing = askTrailing(lens, ask);
  lens.trailing.replaceChildren(...trailing);
  lens.trailing.hidden = trailing.length === 0;
  lens.hints.replaceChildren(...(ask ? ask.hints.map((hint) => (hint.strong ? h("b", {}, hint.text) : text(hint.text))) : []));
  lens.hints.hidden = !ask;
  lens.footerBar.hidden = Boolean(ask);
}

function render(lens) {
  if (!state.client) return;
  const view = JSON.parse(state.client.agent_view(lens.pane, lens.input.value, lens.input.selectionStart ?? lens.input.value.length, lens.reply.value));
  flush();
  if (!view) return;
  lens.view = view;
  const stick = lens.pinned;
  if (view.rows.length) {
    lens.emptyElement?.remove();
    lens.emptyElement = null;
    renderRows(lens, view.rows);
  } else {
    renderRows(lens, []);
    const next = empty(view);
    if (lens.emptyElement) lens.emptyElement.replaceWith(next);
    else lens.area.append(next);
    lens.emptyElement = next;
  }
  lens.root.toggleAttribute("data-running", view.running);
  updateComposer(lens, view);
  if (stick) lens.scroll.scrollTop = lens.scroll.scrollHeight;
  const distance = lens.scroll.scrollHeight - lens.scroll.scrollTop - lens.scroll.clientHeight;
  lens.jump.hidden = !(view.rows.length && distance > JUMP_DISTANCE);
  if (view.running) {
    clearTimeout(lens.tick);
    lens.tick = setTimeout(() => mark(lens.pane), 1000);
  }
}

function mark(pane) {
  dirty.add(pane);
  schedule();
}

function markAll() {
  for (const pane of lenses.keys()) dirty.add(pane);
  schedule();
}

const embedded = new URLSearchParams(location.search).has("embed");

function chat(pane) {
  state.client.agent_open(pane.id);
  flush();
  const lens = createLens(pane);
  lenses.set(pane.id, lens);
  pane.body.append(lens.root);
  pane.element.toggleAttribute("data-chat", true);
  const window = state.tree?.sessions.flatMap((session) => session.windows).find((window) => window.panes.some((item) => item.id === pane.id));
  if (!embedded && window?.activePane === pane.id) lens.input.focus({ preventScroll: true });
  mark(pane.id);
  return {
    destroy() {
      closeModelPopover();
      state.client?.agent_close(pane.id);
      flush();
      clearTimeout(lens.tick);
      lens.root.remove();
      lenses.delete(pane.id);
      pane.element.removeAttribute("data-chat");
    },
  };
}

paneRenderer("agent", chat);

const flipButtons = new Map();

paneHeaderButton((pane) => {
  const info = flipState(pane.id);
  if (!info) {
    flipButtons.delete(pane.id);
    return null;
  }
  let btn = flipButtons.get(pane.id);
  if (!btn) {
    btn = h("button", { "data-penna": "icon-button.toggle" }, icon(info.icon));
    btn.addEventListener("click", (event) => {
      event.stopPropagation();
      flip(pane.id);
    });
    flipButtons.set(pane.id, btn);
  }
  btn.title = info.title;
  btn.firstChild.dataset.icon = info.icon;
  btn.toggleAttribute("data-armed", Boolean(info.armed));
  btn.disabled = Boolean(info.pending);
  return btn;
});

export function flip(pane) {
  if (!state.client?.agent_flip(pane)) return;
  flush();
  markTree();
}

export function chooseFlip(pane, choice) {
  if (!state.client?.agent_flip_choose(pane, choice)) return;
  flush();
  markTree();
}

export function flipState(pane) {
  return state.client ? JSON.parse(state.client.agent_flip_state(pane)) : null;
}

phase("tree", () => {
  if (!state.client) return;
  state.client.agent_prune();
  markAll();
});

phase("after", () => {
  if (!dirty.size) return;
  const pending = [...dirty];
  dirty.clear();
  for (const pane of pending) {
    const lens = lenses.get(pane);
    if (lens?.root.isConnected) render(lens);
  }
});

function stream(event) {
  if (!state.client) return;
  state.client.agent_pump();
  flush();
  const status = state.client.agent_status();
  if (status) emit("status", { text: status });
  mark(event.pane);
}

on("agentUpdates", stream);
on("agentLagged", stream);
on("agentState", stream);
on("hello", () => {
  state.client.agent_connection(true);
  flush();
  markAll();
});
on("attached", () => {
  state.client.agent_connection(true);
  flush();
  markAll();
});
on("disconnected", () => {
  state.client?.agent_connection(false);
  markAll();
});
on("paneRemoved", (event) => {
  state.client.agent_forget(event.pane);
});
on("flip", (event) => {
  if (event.error) emit("status", { text: event.error });
  markTree();
});
on("theme", () => {
  for (const lens of lenses.values()) lens.rowCache.clear();
  markAll();
});

export function providers(force = false) {
  if (!state.client) return null;
  const report = JSON.parse(state.client.agent_providers(force));
  flush();
  return report;
}

export function updateProvider(provider) {
  state.client?.agent_provider_update(provider);
  flush();
}

globalThis.zzAgent = {
  lenses,
  flip,
  chooseFlip,
  render: (pane) => mark(pane),
  providers,
  updateProvider,
  renderMermaid,
  highlightCode,
  renderMarkdown: (html, paneId) => markdown(html, paneId),
};
