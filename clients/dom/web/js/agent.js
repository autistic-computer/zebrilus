import { emit, flush, on, phase, schedule, state } from "./core.js";
import { paneHeaderButton, panes } from "./workspace.js";

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
const TASK_ICONS = { shell: "terminal", agent: "bot", monitor: "inspector" };
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
  return h("button", { "data-zz": zz, title, onclick, ...props }, iconName ? icon(iconName) : null);
}

function copy(value) {
  navigator.clipboard?.writeText(value).catch(() => {});
}

function spinner() {
  return icon("loader", { class: "agent-spin" });
}

function markdown(html) {
  const root = h("div", { class: "agent-md" });
  root.innerHTML = html || "";
  for (const pre of [...root.querySelectorAll("pre")]) {
    const code = pre.querySelector("code");
    const language = (code?.className.match(/language-(\S+)/)?.[1] || "text").toLowerCase();
    const source = code?.textContent ?? pre.textContent;
    const block = h(
      "div",
      { class: "agent-code", "data-zz": "code" },
      h(
        "div",
        { class: "agent-code-chrome" },
        text(language, { class: "agent-code-language" }),
        button("agent.action", "Copy code", "copy", () => copy(source), { class: "agent-code-copy" }),
      ),
      h("pre", {}, source.replace(/\n$/, "")),
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
  for (const code of root.querySelectorAll(":not(pre) > code")) code.dataset.zz = "code";
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
            h("span", { class: "agent-count-badge", "data-zz": "badge", "data-tone": count.failed ? "danger" : null, "data-prominent": count.failed || null }, text(String(count.count))),
          ),
        ),
      ),
    );
  }
  if (tag) row.append(h("span", { class: "agent-tag", "data-zz": "badge", "data-tone": tag.danger ? "danger" : null }, text(tag.text)));
  if (end) row.append(icon(end, { class: "agent-chevron" }));
  return row;
}

function toolRow(lens, tool) {
  const glyph = tool.spin ? spinner() : icon(tool.agent ? "bot" : TOOL_ICONS[tool.tool] || "asterisk");
  return activity({
    glyph,
    tint: tool.tint,
    label: tool.label,
    tag: tool.tag,
    end: tool.opens ? "external-link" : null,
    title: tool.opens ? "Open the output in a pane" : null,
    onclick: tool.opens ? () => act(lens, () => state.client.agent_open_output(lens.pane, tool.id)) : null,
  });
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
    const steps = h("div", { class: "agent-steps", "data-zz": "rule.tree" });
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
    { class: "agent-bubble", "data-zz": "agent.bubble" },
    user.images.length ? h("div", { class: "agent-attachments" }, user.images.map((src) => h("img", { class: "agent-attachment", "data-zz": "agent.attachment", src, alt: "" }))) : null,
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
          "agent.action",
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
  if (row.kind === "notice") {
    return h("div", { class: "agent-notice" }, h("span", { class: "agent-notice-icon" }, icon("info")), h("div", { class: "agent-notice-text" }, markdown(row.html)));
  }
  if (row.kind === "reply") {
    return h(
      "div",
      { class: "agent-reply" },
      userBubble(lens, row.user),
      row.replies.map((reply) =>
        h(
          "div",
          { class: "agent-reply-popover", "data-zz": "inline-panel" },
          reply.side ? h("div", { class: "agent-reply-side" }, "Side answer · not in the conversation") : null,
          markdown(reply.html),
        ),
      ),
    );
  }
  const turn = h("div", { class: "agent-turn" });
  if (row.trace) turn.append(trace(lens, row));
  for (const answer of row.answers) {
    const block = h("div", { class: "agent-answer" }, markdown(answer.html));
    if (answer.copy !== null && answer.copy !== undefined) {
      block.append(h("div", { class: "agent-answer-actions" }, button("agent.action", "Copy message", "copy", () => copy(answer.copy))));
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

function permissionCard(card, actions) {
  const surface = h("div", { class: "agent-card", "data-zz": "inline-panel" });
  const header = h("div", { class: "agent-card-header" }, h("span", { class: "agent-card-glyph" }, icon("triangle-alert")), h("div", { class: "agent-card-title", "aria-label": card.title }, card.title));
  surface.append(header);
  if (card.kind === "wait") {
    surface.append(
      h(
        "div",
        { class: "agent-card-tiles" },
        h(
          "div",
          { class: "agent-option", "data-zz": "agent.option", role: "button", onclick: actions.show },
          h("div", { class: "agent-option-label", "aria-label": "Show terminal" }, "Show terminal"),
          h("span", { class: "agent-option-trailing" }, icon("square-terminal")),
        ),
      ),
    );
    return surface;
  }
  if (card.counter) header.append(h("span", { class: "agent-card-chip", "data-zz": "badge" }, text(card.counter)));
  header.append(
    h(
      "span",
      { class: "agent-card-close" },
      button("button", "Cancel request", "xmark", () => actions.respond(null), { disabled: !card.writable || null }),
    ),
  );
  surface.append(
    h(
      "div",
      { class: "agent-card-tiles" },
      card.options.map((option, index) =>
        h(
          "div",
          {
            class: "agent-option",
            "data-zz": "agent.option",
            role: "button",
            "data-tone": option.danger ? "danger" : null,
            "data-state": !card.writable ? "disabled" : option.selected ? "selected" : null,
            onclick: () => actions.respond(option.id),
            onmouseenter: () => {
              if (!option.selected) actions.select(index);
            },
          },
          h("div", { class: "agent-option-label", "aria-label": option.name }, option.name),
          h(
            "span",
            { class: "agent-option-trailing" },
            option.selected && card.writable ? h("span", { "data-zz": "keycap", "data-tone": option.danger ? "danger" : null }, text("↵")) : null,
            index < 9 ? h("span", { "data-zz": "keycap", "data-tone": option.danger ? "danger" : null }, text(String(index + 1))) : null,
          ),
        ),
      ),
    ),
  );
  return surface;
}

function questionCard(lens, card) {
  const send = (action) => act(lens, () => state.client.agent_question(lens.pane, JSON.stringify(action)));
  const surface = h("div", { class: "agent-card", "data-zz": "inline-panel" });
  const header = h("div", { class: "agent-card-header" }, h("span", { class: "agent-card-glyph" }, icon("triangle-alert")), h("div", { class: "agent-card-title" }, card.title));
  if (card.counter) header.append(h("span", { class: "agent-card-chip", "data-zz": "badge" }, text(card.counter)));
  header.append(h("span", { class: "agent-card-close" }, button("button", "Dismiss", "xmark", () => send({ dismiss: true }), { disabled: !card.writable || null })));
  surface.append(header);
  card.questions.forEach((question, q) => {
    if (q > 0) surface.append(h("div", { class: "agent-card-divider", "data-zz": "rule.list" }));
    surface.append(
      h(
        "div",
        { class: "agent-question" },
        question.header ? h("span", { "data-zz": "badge" }, question.header) : null,
        h("div", { class: "agent-question-text" }, question.question),
      ),
    );
    const tiles = h("div", { class: "agent-card-tiles" });
    question.options.forEach((option, o) => {
      tiles.append(
        h(
          "div",
          {
            class: "agent-option",
            "data-zz": "agent.option",
            role: "button",
            "data-state": option.picked ? "selected" : null,
            onclick: () => send({ pick: [q, o] }),
          },
          h("div", { class: "agent-option-label" }, option.label, option.description ? h("div", { class: "agent-option-detail" }, option.description) : null),
          option.picked ? h("span", { class: "agent-option-trailing" }, icon("check")) : null,
        ),
      );
    });
    if (question.allowOther) {
      const other = h(
        "div",
        { class: "agent-option", "data-zz": "agent.option", role: "button", "data-state": question.other ? "selected" : null, onclick: () => send({ other: q }) },
        h("div", { class: "agent-option-label" }, "Other"),
      );
      tiles.append(other);
      if (question.other) {
        const field = h("input", { class: "agent-other", "data-zz": "field", placeholder: "Type your answer" });
        field.addEventListener("input", () => state.client.agent_question(lens.pane, JSON.stringify({ otherText: [q, field.value] })));
        field.addEventListener("keydown", (event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            send({ submit: true });
          }
        });
        tiles.append(field);
      }
    }
    surface.append(tiles);
  });
  surface.append(
    h(
      "div",
      { class: "agent-card-footer" },
      card.problem ? h("span", { class: "agent-card-problem" }, card.problem) : null,
      h("button", { "data-zz": "button", "data-prominent": true, disabled: !card.complete || !card.writable || null, onclick: () => send({ submit: true }) }, "Submit"),
    ),
  );
  return surface;
}

function tray(lens, tray, writable) {
  const box = h("div", { class: "agent-tray" });
  if (tray.plan?.open) {
    box.append(
      h(
        "div",
        { class: "agent-tray-panel", "data-zz": "inline-panel" },
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
  } else if (tray.tasks?.open) {
    box.append(
      h(
        "div",
        { class: "agent-tray-panel", "data-zz": "inline-panel" },
        tray.tasks.rows.map((task) =>
          h(
            "div",
            {
              class: "agent-task",
              "data-zz": task.toolCall ? "menu-item" : null,
              onclick: task.toolCall ? () => reveal(lens, task.toolCall) : null,
            },
            icon(TASK_ICONS[task.kind] || "cpu"),
            h("span", { class: "agent-task-text" }, task.description),
            button("button", "Stop", "stop", (event) => {
              event.stopPropagation();
              act(lens, () => state.client.agent_stop_task(lens.pane, task.id));
            }, { class: "agent-task-stop", disabled: !writable || null }),
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
        { class: "agent-plan-chip", "data-zz": "agent.tray", "data-state": tray.plan.open ? "current" : null, role: "button", onclick: () => act(lens, () => state.client.agent_tray(lens.pane, "plan")) },
        icon("circle-check"),
        text(tray.plan.label),
        tray.plan.current ? text(tray.plan.current, { class: "agent-plan-current" }) : null,
        icon(tray.plan.open ? "chevron-down" : "chevron-up", { class: "agent-plan-chevron" }),
      ),
    );
  }
  bar.append(chip);
  if (tray.tasks) {
    bar.append(
      h(
        "button",
        {
          class: "agent-tasks-chip",
          "data-zz": "agent.tray",
          "aria-label": tray.tasks.label,
          title: tray.tasks.open ? "Hide background tasks" : "Show background tasks",
          onclick: () => act(lens, () => state.client.agent_tray(lens.pane, "tasks")),
        },
        spinner(),
        tray.tasks.label,
      ),
    );
  }
  box.append(bar);
  return box;
}

function errorCard(error) {
  return h(
    "div",
    { class: "agent-alert", "data-zz": "inline-panel" },
    h("span", { class: "agent-alert-icon" }, icon("triangle-alert")),
    h("div", { class: "agent-alert-copy" }, error.stopped ? h("div", { class: "agent-alert-title" }, "The agent stopped") : null, h("div", { class: "agent-alert-text", "data-titled": error.stopped || null }, error.text)),
  );
}

function completions(lens, view) {
  const list = h("div", { class: "agent-completions", "data-zz": "inline-panel" });
  view.completions.forEach((completion, index) => {
    const row = h(
      "div",
      {
        class: "agent-completion",
        "data-zz": "menu-item",
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

function usageRing(usage) {
  const radius = 7;
  const length = 2 * Math.PI * radius;
  const ring = `<svg width="16" height="16" viewBox="0 0 16 16"><circle cx="8" cy="8" r="${radius}" fill="none" stroke-width="2" class="agent-usage-track"/><circle cx="8" cy="8" r="${radius}" fill="none" stroke-width="2" class="agent-usage-fill" stroke-dasharray="${(usage.fraction * length).toFixed(2)} ${length.toFixed(2)}" transform="rotate(-90 8 8)"/></svg>`;
  const meter = h("div", { class: "agent-usage", role: "progressbar", "aria-label": usage.tooltip, title: usage.tooltip });
  meter.innerHTML = ring;
  return meter;
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

function reveal(lens, toolCall) {
  const key = state.client.agent_reveal(lens.pane, toolCall);
  const row = key && lens.list.querySelector(`[data-key="${key}"]`);
  if (row) {
    lens.pinned = false;
    row.scrollIntoView({ block: "start", behavior: "smooth" });
  }
}

function submit(lens) {
  const value = lens.input.value;
  const sent = state.client.agent_submit(lens.pane, value);
  flush();
  if (sent) {
    lens.input.value = "";
    grow(lens);
    lens.pinned = true;
  }
  mark(lens.pane);
}

function grow(lens) {
  const input = lens.input;
  input.style.height = "auto";
  const line = parseFloat(getComputedStyle(input).lineHeight) || 16;
  const padding = parseFloat(getComputedStyle(input).paddingTop) * 2 || 0;
  input.style.height = `${Math.min(input.scrollHeight, line * 8 + padding)}px`;
}

function keyName(event) {
  return { ArrowUp: "up", ArrowDown: "down", Escape: "escape", Enter: "enter" }[event.key] || event.key;
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
  const jump = button("agent.jump", "Jump to latest", "arrow-down", () => {
    lens.pinned = true;
    scroll.scrollTo({ top: scroll.scrollHeight, behavior: "smooth" });
  }, { class: "agent-jump" });
  area.append(jump);
  const prefix = h("div", { class: "agent-prefix" });
  const mention = button("agent.send", "Mention a host, session or file", "plus", () => {
    const at = input.selectionStart;
    input.setRangeText("@", at, input.selectionEnd, "end");
    input.focus();
    mark(lens.pane);
  });
  const hint = h("div", { class: "agent-hint" });
  const usage = h("div", { class: "agent-usage-slot" });
  const action = h("button", { "data-zz": "agent.send", "data-prominent": true, "data-tone": "primary", class: "agent-action" }, icon("arrow-up"));
  action.addEventListener("click", () => {
    if (lens.view?.action === "stop") act(lens, () => state.client.agent_cancel(lens.pane));
    else submit(lens);
  });
  const pill = h("div", { class: "agent-pill", "data-zz": "agent.composer" }, mention, h("div", { class: "agent-field" }, input, hint), usage, action);
  const upper = h("div", { class: "agent-upper" }, h("div", { class: "agent-column agent-stack" }, prefix, h("div", { class: "agent-pill-back" }, pill)));
  const git = h("div", { class: "agent-column agent-git" });
  const composer = h("div", { class: "agent-composer" }, upper, h("div", { class: "agent-footer" }, git));
  const root = h("div", { class: "agent-lens" }, area, composer);
  Object.assign(lens, { root, list, scroll, area, jump, prefix, pill, upper, hint, usage, action, git, composer, mention });

  scroll.addEventListener("scroll", () => {
    const distance = scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight;
    lens.pinned = distance <= STICK_DISTANCE;
    jump.hidden = !(lens.rowCache.size && distance > JUMP_DISTANCE);
  });
  input.addEventListener("input", () => {
    grow(lens);
    mark(lens.pane);
  });
  input.addEventListener("focus", () => pill.setAttribute("data-state", "focused"));
  input.addEventListener("blur", () => pill.removeAttribute("data-state"));
  input.addEventListener("keyup", (event) => {
    if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) mark(lens.pane);
  });
  input.addEventListener("keydown", (event) => {
    if (event.isComposing) return;
    const view = lens.view;
    const hasCompletions = Boolean(view?.completions.length);
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
    if (["ArrowUp", "ArrowDown", "Escape"].includes(event.key) || (/^[1-9]$/.test(event.key) && !input.value.trim())) {
      if (event.metaKey || event.altKey || event.ctrlKey) return;
      if (state.client.agent_key(lens.pane, keyName(event), !input.value.trim(), true)) {
        event.preventDefault();
        flush();
        mark(lens.pane);
      }
    }
  });
  root.addEventListener("keydown", (event) => {
    if (event.target === input || event.target.closest("input, textarea")) return;
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
    input.focus({ preventScroll: true });
  });
  return lens;
}

function updateComposer(lens, view) {
  lens.input.placeholder = view.placeholder;
  lens.composer.hidden = false;
  lens.pill.parentElement.hidden = view.watchOnly;
  const prefix = [];
  if (view.card) {
    prefix.push(
      view.card.kind === "question"
        ? questionCard(lens, view.card)
        : permissionCard(view.card, {
            respond: (option) => act(lens, () => state.client.agent_respond(lens.pane, view.card.requestId, option)),
            select: (index) => act(lens, () => state.client.agent_permission_select(lens.pane, index)),
            show: () => toggle(lens.pane),
          }),
    );
  }
  if (view.queued) {
    prefix.push(
      h(
        "div",
        { class: "agent-queue" },
        h(
          "button",
          {
            "data-zz": "button.quiet",
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
  if (view.tray) prefix.push(tray(lens, view.tray, view.writable));
  if (view.error) prefix.push(errorCard(view.error));
  if (view.completions.length) prefix.push(completions(lens, view));
  lens.prefix.replaceChildren(...prefix);
  lens.composer.toggleAttribute("data-prefix", prefix.length > 0);
  lens.hint.textContent = view.hint || "";
  lens.hint.hidden = !view.hint;
  lens.usage.replaceChildren(...(view.usage ? [usageRing(view.usage)] : []));
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

function render(lens) {
  if (!state.client) return;
  const view = JSON.parse(state.client.agent_view(lens.pane, lens.input.value, lens.input.selectionStart ?? lens.input.value.length));
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
  emit("driver");
  schedule();
}

function attach(pane) {
  const host = panes.get(pane);
  const lens = lenses.get(pane);
  if (!host || !lens) return;
  if (lens.root.parentElement !== host.body) host.body.append(lens.root);
  host.element.toggleAttribute("data-lens", true);
}

function open(pane) {
  if (!state.client.agent_open(pane)) return;
  flush();
  const host = panes.get(pane);
  if (!host) return;
  const lens = createLens(host);
  lenses.set(pane, lens);
  attach(pane);
  lens.input.focus({ preventScroll: true });
  mark(pane);
}

function close(pane) {
  state.client.agent_close(pane);
  flush();
  const lens = lenses.get(pane);
  lens?.root.remove();
  clearTimeout(lens?.tick);
  lenses.delete(pane);
  panes.get(pane)?.element.removeAttribute("data-lens");
}

export function toggle(pane) {
  if (lenses.has(pane)) close(pane);
  else open(pane);
  const window = state.tree?.sessions.flatMap((session) => session.windows).find((window) => window.panes.some((item) => item.id === pane));
  if (window && window.activePane !== pane) state.client.command("select-pane", ["-t", `%${pane}`]);
  flush();
  syncHeaders();
}

const lensButtons = new WeakMap();

paneHeaderButton((pane) => {
  let lensButton = lensButtons.get(pane);
  if (!lensButton) {
    lensButton = button("pane.header.lens", "Show agent view", "eye", (event) => {
      event.stopPropagation();
      toggle(pane.id);
    });
    lensButtons.set(pane, lensButton);
  }
  lensButton.hidden = !state.client?.agent_provider(pane.id);
  return lensButton;
});

function syncHeaders() {
  for (const [id, pane] of panes) {
    const lensButton = pane.element.querySelector("[data-zz='pane.header.lens']");
    if (!lensButton) continue;
    lensButton.hidden = !state.client?.agent_provider(id);
    const shown = lenses.has(id);
    lensButton.title = shown ? "Show terminal" : "Show agent view";
    lensButton.toggleAttribute("data-open", shown);
    if (shown) lensButton.dataset.state = "selected";
    else delete lensButton.dataset.state;
  }
}

phase("tree", () => {
  if (!state.client) return;
  state.client.agent_prune();
  for (const pane of [...lenses.keys()]) {
    if (!state.client.agent_lens_open(pane) || !panes.has(pane)) {
      lenses.get(pane)?.root.remove();
      lenses.delete(pane);
      panes.get(pane)?.element.removeAttribute("data-lens");
    }
  }
  syncHeaders();
  for (const pane of lenses.keys()) attach(pane);
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
  emit("driver");
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
  const lens = lenses.get(event.pane);
  lens?.root.remove();
  lenses.delete(event.pane);
});
on("theme", markAll);


const driver = { sheet: null, shown: 0, dirty: false };

function clock() {
  const now = new Date();
  return `${now.getHours()}:${String(now.getMinutes()).padStart(2, "0")}`;
}

function bubble(message) {
  const mine = message.kind === "mine";
  const body = mine ? h("div", { class: "driver-text" }, message.text) : markdown(message.html);
  const stamp = h(
    "span",
    { class: "driver-stamp" },
    message.time,
    mine ? icon("check-read", { class: "driver-tick", "data-read": message.read || null }) : null,
  );
  return h(
    "div",
    { class: "driver-line", "data-mine": mine || null },
    h(
      "div",
      { class: "driver-bubble", "data-zz": mine ? "chat.bubble.mine" : "chat.bubble", "data-tone": mine ? "accent" : null, "data-prominent": mine || null },
      message.sender ? h("div", { class: "driver-sender" }, message.sender.name) : null,
      body,
      stamp,
    ),
  );
}

function typing(steps) {
  const shown = steps.slice(-3);
  const current = shown.pop();
  return h(
    "div",
    { class: "driver-line" },
    h(
      "div",
      { class: "driver-typing", "data-zz": "chat.bubble" },
      shown.map((step) => h("div", { class: "driver-step" }, icon("check"), h("span", {}, step))),
      h("div", { class: "driver-now" }, h("span", { class: "driver-dots" }, h("i"), h("i"), h("i")), current ? h("span", {}, current) : null),
    ),
  );
}

function createDriverSheet() {
  const input = h("textarea", { class: "agent-input", rows: "1", placeholder: "Message the driver…", spellcheck: "false" });
  const action = h("button", { "data-zz": "agent.send", "data-prominent": true, "data-tone": "primary", class: "agent-action" }, icon("arrow-up"));
  const usage = h("div", { class: "agent-usage-slot" });
  const mention = button("agent.send", "Mention a host, session or file", "plus", () => {
    input.setRangeText("@", input.selectionStart, input.selectionEnd, "end");
    input.focus();
  });
  const pill = h("div", { class: "agent-pill", "data-zz": "agent.composer" }, mention, h("div", { class: "agent-field" }, input), usage, action);
  const thread = h("div", { class: "driver-thread" });
  const scroll = h("div", { class: "driver-scroll" }, thread);
  const threadWrap = h("div", { class: "driver-thread-wrap" }, scroll);
  const ask = h("div", { class: "driver-ask" });
  const heading = h("div", { class: "driver-heading" });
  const close = h("button", { "data-zz": "button.quiet", class: "driver-close", title: "Close", onclick: closeDriverChat }, icon("xmark"));
  const panel = h(
    "div",
    { class: "driver-panel", "data-zz": "drawer.driver", onclick: (event) => event.stopPropagation() },
    h("div", { class: "driver-grabber-row" }, h("div", { class: "driver-grabber", "data-zz": "grabber" })),
    h("div", { class: "driver-header", "data-zz": "phone.bar" }, heading, close),
    h("div", { class: "driver-body" }, threadWrap, ask, h("div", { class: "driver-composer" }, pill)),
  );
  const root = h(
    "div",
    { class: "driver-layer", onmouseup: (event) => event.stopPropagation() },
    h("div", { class: "driver-scrim", onclick: closeDriverChat }),
    h("div", { class: "driver-frame" }, panel),
  );
  const sheet = { root, input, action, usage, thread, scroll, threadWrap, ask, heading, view: null };
  const send = () => {
    if (sheet.view?.running && !input.value.trim()) {
      state.client.driver_stop();
    } else if (input.value.trim() && state.client.driver_send(input.value)) {
      input.value = "";
      scroll.scrollTop = scroll.scrollHeight;
    }
    flush();
    markDriver();
  };
  action.addEventListener("click", send);
  input.addEventListener("input", () => {
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 16.25 * 6 + 12)}px`;
    markDriver();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeDriverChat();
    }
  });
  return sheet;
}

function renderDriver() {
  const sheet = driver.sheet;
  const view = JSON.parse(state.client.driver_view(Date.now(), clock(), "This computer", Boolean(sheet)));
  flush();
  emit("driverState", { unread: view.unread, running: view.running, present: Boolean(view.driver) });
  if (!sheet) return;
  sheet.view = view;
  if (view.driver) {
    sheet.heading.replaceChildren(
      h(
        "div",
        { class: "driver-place", role: "button", onclick: showDriverTerminal },
        text("Driver", { class: "driver-title" }),
        h("div", { class: "driver-host" }, text(view.driver.host), icon("chevron-right")),
      ),
    );
  } else {
    sheet.heading.replaceChildren(text("Driver", { class: "driver-title" }));
  }
  const bubbles = view.messages.map(bubble);
  if (view.typing) bubbles.push(typing(view.typing));
  sheet.thread.replaceChildren(...bubbles);
  const depth = view.messages.length * 5 + (view.typing ? 1 + Math.min(view.typing.length, 3) : 0);
  if (depth > driver.shown) sheet.scroll.scrollTop = sheet.scroll.scrollHeight;
  driver.shown = depth;
  sheet.threadWrap.querySelector(".driver-asleep")?.remove();
  if (view.asleep) {
    sheet.threadWrap.append(
      h(
        "div",
        { class: "driver-asleep" },
        h("div", { class: "driver-note" }, view.asleep.note),
        h(
          "button",
          {
            "data-zz": "button",
            "data-prominent": true,
            "data-tone": "primary",
            "aria-label": view.asleep.label,
            disabled: view.asleep.waking || null,
            onclick: () => {
              state.client.driver_wake(view.asleep.force, Date.now());
              flush();
              markDriver();
            },
          },
          view.asleep.waking ? spinner() : null,
          view.asleep.label,
        ),
      ),
    );
  }
  const permission = view.permission;
  sheet.ask.replaceChildren(
    ...(permission
      ? [
          permissionCard(
            {
              kind: "permission",
              requestId: permission.requestId,
              title: permission.title,
              writable: true,
              options: permission.options.map((option, index) => ({ ...option, selected: index === 0 })),
            },
            {
              respond: (option) => {
                state.client.driver_respond(permission.requestId, option);
                flush();
                markDriver();
              },
              select: () => {},
              show: showDriverTerminal,
            },
          ),
        ]
      : []),
  );
  sheet.usage.replaceChildren(...(view.usage ? [usageRing(view.usage)] : []));
  const typed = Boolean(sheet.input.value.trim());
  const stop = view.running && !typed;
  sheet.action.title = stop ? "Stop the current turn" : "Send message";
  sheet.action.firstChild.dataset.icon = stop ? "stop" : "arrow-up";
  sheet.action.disabled = !(stop || typed);
  if (view.running) {
    clearTimeout(driver.tick);
    driver.tick = setTimeout(markDriver, 1000);
  }
}

function markDriver() {
  driver.dirty = true;
  schedule();
}

function showDriverTerminal() {
  const target = state.client.driver_terminal();
  closeDriverChat();
  if (!target) return;
  const { session, pane } = JSON.parse(target);
  state.client.attach(`$${session}`);
  state.client.command("select-pane", ["-t", `%${pane}`]);
  flush();
}

export function openDriverChat() {
  if (driver.sheet) return;
  driver.sheet = createDriverSheet();
  driver.shown = 0;
  document.body.append(driver.sheet.root);
  driver.sheet.input.focus();
  markDriver();
}

export function closeDriverChat() {
  if (!driver.sheet) return;
  driver.sheet.root.remove();
  driver.sheet = null;
  clearTimeout(driver.tick);
}

phase("after", () => {
  if (!driver.dirty || !state.client) return;
  driver.dirty = false;
  renderDriver();
});

on("driver", markDriver);
on("openDriverChat", openDriverChat);
on("closeDriverChat", closeDriverChat);

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

globalThis.zzAgent = { lenses, toggle, render: (pane) => mark(pane), openDriverChat, closeDriverChat, providers, updateProvider };
