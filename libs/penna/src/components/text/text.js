(() => {
  const CELL_PAD = 16;
  const CELL_MIN = 48;
  const CELL_MAX = 480;
  const WRAP_MAX = 1200;

  let probe;

  function runs(cell) {
    const out = [];
    const walk = (node, mark) => {
      if (node.nodeType === Node.TEXT_NODE) {
        if (node.data) out.push({ text: node.data, ...mark });
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return;
      const next = { ...mark };
      if (node.matches("strong, b")) next.bold = true;
      if (node.matches("em, i")) next.italic = true;
      if (node.matches("code")) next.family = getComputedStyle(node).fontFamily;
      node.childNodes.forEach((child) => walk(child, next));
    };
    cell.childNodes.forEach((child) => walk(child, {}));
    return out;
  }

  function cellWidth(cell, size, family) {
    if (!probe) {
      probe = document.createElement("span");
      probe.style.cssText = "position:absolute;left:-99999px;top:0;white-space:pre;visibility:hidden;font-kerning:normal;font-optical-sizing:none";
      document.body.append(probe);
    }
    const parts = runs(cell);
    const text = parts.map((part) => part.text).join("");
    const lead = text.length - text.trimStart().length;
    const end = text.trimEnd().length;
    let at = 0;
    const spans = [];
    for (const part of parts) {
      const from = Math.max(at, lead) - at;
      const to = Math.min(at + part.text.length, end) - at;
      at += part.text.length;
      if (to <= from) continue;
      const span = document.createElement("span");
      span.style.fontFamily = part.family ?? family;
      span.style.fontSize = size;
      span.style.fontWeight = part.bold ? "700" : "400";
      span.style.fontStyle = part.italic ? "italic" : "normal";
      span.textContent = part.text.slice(from, to);
      spans.push(span);
    }
    probe.replaceChildren(...spans);
    return probe.getBoundingClientRect().width;
  }

  function sizeTable(table) {
    const scroll = table.hasAttribute("data-scroll");
    const rows = [...table.querySelectorAll(".md-row")];
    const view = table.closest(".penna-md");
    const style = getComputedStyle(view);
    const count = Math.max(...rows.map((row) => row.children.length));
    const widths = Array(count).fill(CELL_MIN);
    rows.forEach((row) =>
      [...row.children].forEach((cell, index) => {
        widths[index] = Math.max(widths[index], cellWidth(cell, style.fontSize, style.fontFamily) + CELL_PAD);
      }),
    );
    const columns = widths.map((width) => Math.min(width, scroll ? CELL_MAX : WRAP_MAX));
    rows.forEach((row) =>
      [...row.children].forEach((cell, index) => cell.style.setProperty("--md-w", columns[index] ?? CELL_MIN)),
    );
    if (scroll) {
      const figure = table.querySelector(".md-figure");
      figure.style.setProperty("--md-total", `${columns.reduce((sum, width) => sum + width, 0)}px`);
    }
  }

  function selectAll(view) {
    view.dataset.selected = "";
    const selection = getSelection();
    selection.removeAllRanges();
    const range = document.createRange();
    range.selectNodeContents(view);
    selection.addRange(range);
  }

  function stream(view) {
    const tail = view.querySelector("template[data-tail]");
    if (!tail) return;
    requestAnimationFrame(() => {
      view.append(tail.content.cloneNode(true));
      tail.remove();
      settle();
    });
  }

  const metrics = new Map();

  function fontMetrics(family) {
    if (metrics.has(family)) return metrics.get(family);
    const box = document.createElement("div");
    box.style.cssText = `position:absolute;left:-99999px;top:0;font:1000px/normal ${family};white-space:pre`;
    const text = document.createElement("span");
    text.textContent = "x";
    const mark = document.createElement("span");
    mark.style.cssText = "display:inline-block;width:0;height:0;vertical-align:baseline";
    box.append(text, mark);
    document.body.append(box);
    const span = text.getBoundingClientRect();
    const base = mark.getBoundingClientRect().top;
    box.remove();
    const value = { ascent: (base - span.top) / 1000, descent: (span.bottom - base) / 1000 };
    metrics.set(family, value);
    return value;
  }

  function device(value) {
    const scale = devicePixelRatio || 1;
    const scaled = value * scale;
    return (Math.sign(scaled) * Math.ceil(Math.abs(scaled) - 0.5)) / scale;
  }

  const TEXT = ".penna-md :is(p, h1, h2, h3, h4, h5, h6, .md-prefix, .md-cell)";

  function baselines(root = document) {
    metrics.clear();
    const blocks = [...root.querySelectorAll(TEXT)];
    blocks.forEach((block) => block.style.setProperty("top", "0px"));
    const plans = blocks.map((block) => {
      const style = getComputedStyle(block);
      const size = parseFloat(style.fontSize);
      const line = parseFloat(style.lineHeight);
      let { ascent, descent } = fontMetrics(style.fontFamily);
      const code = block.querySelector("code");
      if (code) {
        const mono = fontMetrics(getComputedStyle(code).fontFamily);
        ascent = Math.max(ascent, mono.ascent);
        descent = Math.max(descent, mono.descent);
      }
      const mark = document.createElement("span");
      mark.style.cssText = "display:inline-block;width:0;height:0;vertical-align:baseline";
      block.prepend(mark);
      return { block, size, line, ascent, descent, mark };
    });
    plans.forEach((plan) => {
      const { block, size, line, ascent, descent, mark } = plan;
      const rect = block.getBoundingClientRect();
      const style = getComputedStyle(block);
      const top = rect.top + scrollY + (parseFloat(style.paddingTop) || 0) + (parseFloat(style.borderTopWidth) || 0);
      const target = device(top + (line - (ascent + descent) * size) / 2 + ascent * size);
      const drawn = device(mark.getBoundingClientRect().top + scrollY);
      plan.shift = target - drawn;
    });
    plans.forEach(({ block, mark, shift }) => {
      mark.remove();
      block.style.setProperty("top", `${shift}px`);
      block.style.setProperty("--md-shift", `${shift}px`);
    });
  }

  const INLINE_CODE_RADIUS = 6;

  function shape(root = document) {
    const fraction = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-corner-fraction")) || 0.45;
    root.querySelectorAll(".penna-md :not(pre) > code, .penna-md :not(pre) > :is(strong, em, a) > code").forEach((code) => {
      const rect = code.getClientRects()[0];
      if (!rect) return;
      const cap = Math.min(rect.width, rect.height) * fraction;
      code.style.setProperty("--md-r", `${(cap * Math.tanh(INLINE_CODE_RADIUS / cap)).toFixed(2)}px`);
    });
  }

  function shapeChecks(root = document) {
    const style = getComputedStyle(document.documentElement);
    const fraction = parseFloat(style.getPropertyValue("--penna-corner-fraction")) || 0.45;
    const radius = (parseFloat(style.getPropertyValue("--penna-radius")) || 0) / 2;
    root.querySelectorAll(".penna-md .md-check").forEach((check) => {
      const cap = Math.min(check.offsetWidth, check.offsetHeight) * fraction;
      if (!cap || radius <= 0) return check.style.removeProperty("--md-check-r");
      check.style.setProperty("--md-check-r", `${(cap * Math.tanh(radius / cap)).toFixed(2)}px`);
    });
  }

  function settle() {
    document.querySelectorAll(".penna-md .md-table").forEach(sizeTable);
    shape();
    shapeChecks();
    pennaKit.ceilText(document, ".penna-md .md-prefix");
    baselines();
  }

  document.addEventListener("click", (event) => {
    const copy = event.target.closest(".penna-md [data-copy]");
    if (copy) {
      const text = copy.closest("pre")?.querySelector("code")?.textContent ?? "";
      navigator.clipboard?.writeText(text).catch(() => {});
    }
  });

  document.addEventListener("keydown", (event) => {
    const view = event.target.closest?.(".penna-md[data-selectable]");
    if (view && event.key === "a" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      selectAll(view);
    }
  });

  document.addEventListener("mousedown", (event) => {
    if (event.target.closest?.("button")) return;
    document.querySelectorAll(".penna-md[data-selectable][data-selected]").forEach((view) => delete view.dataset.selected);
  });

  document.querySelectorAll(".penna-md[data-selectable]").forEach((view) => {
    view.tabIndex = 0;
  });
  document.querySelectorAll(".penna-md[data-selectable][data-selected]").forEach(selectAll);

  const FACES = ["400 16px 'Inter Variable'", "italic 400 16px 'Inter Variable'", "700 16px 'Inter Variable'", "400 13px Lilex", "700 13px Lilex"];

  settle();
  document.querySelectorAll(".penna-md[data-streaming]").forEach(stream);
  Promise.all(FACES.map((face) => document.fonts.load(face).catch(() => {})))
    .then(() => document.fonts.ready)
    .then(settle);
  document.fonts.addEventListener("loadingdone", settle);
  addEventListener("load", settle);
  new MutationObserver(() => requestAnimationFrame(settle)).observe(document.documentElement, { attributeFilter: ["data-penna-theme", "data-penna-mode"] });
})();
