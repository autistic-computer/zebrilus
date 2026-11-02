(() => {
const KEYWORDS = new Set("as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return static struct super trait type unsafe use where while".split(" "));
const PRIMITIVES = new Set("str bool char u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64".split(" "));
const CURSOR_WIDTH = 2;
const NEWLINE_SELECTION_WIDTH = 4;
const GUTTER_PADDING = 5;
const CORNER = 0.45;
const TAB = 4;

function rustTokens(text) {
  const out = [];
  const pattern = /(\/\/[^\n]*)|("(?:\\.|[^"\\])*")|('(?:\\.|[^\\'])')|('[A-Za-z_]\w*)|(\d[\d_]*(?:\.\d+)?)|([A-Za-z_]\w*)|(\s+)|([\s\S])/g;
  const stack = [];
  let inUse = false;
  let previous = "";
  let match;
  while ((match = pattern.exec(text))) {
    const [token, comment, string, char, lifetime, number, ident, space, punct] = match;
    const after = text.slice(pattern.lastIndex);
    let kind = "";
    if (comment) kind = "m";
    else if (string || char) kind = "s";
    else if (number) kind = "n";
    else if (lifetime) kind = "";
    else if (ident) {
      if (ident === "true" || ident === "false") kind = "b";
      else if (KEYWORDS.has(ident)) kind = "k";
      else if (PRIMITIVES.has(ident)) kind = "t";
      else if (/^[A-Z]/.test(ident)) kind = inUse ? "c" : "t";
      else if (/^\s*[(!]/.test(after) && !/^\s*!=/.test(after)) kind = "f";
      else if (previous === ".") kind = "p";
      else if (/^\s*:(?!:)/.test(after) && stack.at(-1) === "{") kind = "p";
      if (ident === "use") inUse = true;
    } else if (punct) {
      if ("{([".includes(punct)) stack.push(punct);
      else if ("})]".includes(punct)) stack.pop();
      else if (punct === ";") inUse = false;
    }
    if (!space) previous = punct || "";
    out.push([token, kind]);
  }
  return out;
}

function jsonTokens(text) {
  const out = [];
  const pattern = /("(?:\\.|[^"\\])*")|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|(true|false|null)|([\s\S])/g;
  let match;
  while ((match = pattern.exec(text))) {
    const [token, string, number, constant] = match;
    let kind = "";
    if (string) kind = /^[^\s",:{}[\]]*\s*:/.test(text.slice(pattern.lastIndex)) ? "p" : "s";
    else if (number) kind = "n";
    else if (constant) kind = "b";
    out.push([token, kind]);
  }
  return out;
}

const LANGUAGES = { rust: rustTokens, json: jsonTokens };

function tokenize(text, language) {
  const tokens = (LANGUAGES[language] ?? ((value) => [[value, ""]]))(text);
  const lines = [[]];
  for (const [token, kind] of tokens) {
    token.split("\n").forEach((part, index) => {
      if (index) lines.push([]);
      if (!part) return;
      const line = lines.at(-1);
      const last = line.at(-1);
      if (last && last[1] === kind) last[0] += part;
      else line.push([part, kind]);
    });
  }
  return lines;
}

function lineHtml(tokens) {
  const escape = (value) => value.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
  return tokens.map(([text, kind]) => (kind ? `<span class="ce-${kind}">${escape(text)}</span>` : escape(text))).join("");
}

function resolveRadius(radius, width, height) {
  const cap = Math.min(width, height) * CORNER;
  return cap > 0 ? cap * Math.tanh(radius / cap) : 0;
}

function isWord(char) {
  return /\w/.test(char ?? "");
}

class CodeEditor {
  constructor(root) {
    this.root = root;
    const source = root.dataset.source && document.getElementById(root.dataset.source);
    this.text = source ? source.textContent.replace(/^\n/, "") : root.textContent;
    this.language = root.dataset.language ?? "text";
    this.disabled = root.getAttribute("aria-disabled") === "true";
    this.vim = root.hasAttribute("data-vim") ? { mode: "normal", anchor: 0, pending: "" } : null;
    const [anchor, head] = (root.dataset.selection ?? `${this.text.length}`).split(",").map(Number);
    this.anchor = anchor;
    this.head = head ?? anchor;
    this.follow = root.hasAttribute("data-selection");
    const scroll = root.dataset.scroll?.split(",").map(Number);
    this.scroll = { x: 0, y: 0 };
    this.initialScroll = scroll;
    if (this.vim && root.dataset.vimVisual) {
      this.vim.mode = "visual";
      this.vim.anchor = Number(root.dataset.vimVisual);
      this.anchor = this.head;
    }
    this.history = [];
    this.future = [];
    this.goal = null;
    this.build();
    this.bind();
    this.render();
  }

  build() {
    this.root.textContent = "";
    if (!this.disabled && !this.root.hasAttribute("tabindex")) this.root.tabIndex = 0;
    if (this.disabled) this.root.tabIndex = -1;
    const make = (cls, parent = this.root) => parent.appendChild(Object.assign(document.createElement("div"), { className: cls }));
    this.surface = make("ce-surface");
    this.gutter = make("ce-gutter");
    const content = make("ce-content");
    this.scroller = make("ce-scroller", content);
    this.active = make("ce-active", this.scroller);
    this.quadLayer = make("ce-quads", this.scroller);
    this.linesEl = make("ce-lines", this.scroller);
    this.caret = make("ce-caret", this.scroller);
    this.numbers = make("ce-numbers");
    this.probe = Object.assign(document.createElement("span"), { className: "ce-probe" });
    this.root.append(this.probe);
  }

  get lineNumbers() {
    return this.root.dataset.lineNumbers ?? "absolute";
  }

  get wrap() {
    return this.root.hasAttribute("data-wrap");
  }

  bind() {
    const root = this.root;
    root.addEventListener("focus", () => {
      root.dataset.state = "focused";
      this.restartBlink();
    });
    root.addEventListener("blur", () => {
      if (document.activeElement !== root) root.removeAttribute("data-state");
    });
    root.addEventListener("mousedown", (event) => this.onMouseDown(event));
    root.addEventListener("wheel", (event) => this.onWheel(event), { passive: false });
    root.addEventListener("keydown", (event) => this.onKey(event));
    root.addEventListener("paste", (event) => {
      if (this.disabled) return;
      event.preventDefault();
      this.insert(event.clipboardData.getData("text/plain"));
    });
    root.addEventListener("copy", (event) => {
      const [start, end] = this.range();
      if (start === end) return;
      event.preventDefault();
      event.clipboardData.setData("text/plain", this.text.slice(start, end));
    });
    root.addEventListener("cut", (event) => {
      const [start, end] = this.range();
      if (start === end || this.disabled) return;
      event.preventDefault();
      event.clipboardData.setData("text/plain", this.text.slice(start, end));
      this.insert("");
    });
    new ResizeObserver(() => this.layout()).observe(root);
  }

  range() {
    return [Math.min(this.anchor, this.head), Math.max(this.anchor, this.head)];
  }

  highlightRange() {
    if (this.vim?.mode === "visual") {
      const start = Math.min(this.vim.anchor, this.head);
      const end = Math.max(this.vim.anchor, this.head);
      return [start, Math.min(this.text.length, this.nextBoundary(end))];
    }
    return this.range();
  }

  nextBoundary(offset) {
    return Math.min(this.text.length, offset + 1);
  }

  lineStarts() {
    const starts = [0];
    for (let i = 0; i < this.text.length; i++) if (this.text[i] === "\n") starts.push(i + 1);
    return starts;
  }

  lineOf(offset) {
    const starts = this.starts;
    let low = 0;
    let high = starts.length - 1;
    while (low < high) {
      const mid = (low + high + 1) >> 1;
      if (starts[mid] <= offset) low = mid;
      else high = mid - 1;
    }
    return low;
  }

  lineEnd(index) {
    return index + 1 < this.starts.length ? this.starts[index + 1] - 1 : this.text.length;
  }

  render() {
    this.starts = this.lineStarts();
    const lines = tokenize(this.text, this.language);
    this.linesEl.innerHTML = lines.map((tokens, index) => `<div class="ce-line" data-line="${index}">${lineHtml(tokens)}</div>`).join("");
    this.numbers.innerHTML = lines.map(() => `<div class="ce-number"></div>`).join("");
    this.layout();
  }

  layout() {
    const root = this.root;
    if (!root.isConnected || !root.clientWidth || !this.ready) return;
    const style = getComputedStyle(root);
    this.lh = parseFloat(getComputedStyle(this.linesEl).lineHeight);
    const digits = Math.max(2, String(this.starts.length).length);
    this.probe.textContent = "8".repeat(digits);
    const gutter = this.lineNumbers === "off" ? 0 : this.probe.getBoundingClientRect().width + GUTTER_PADDING * 2;
    this.gutterWidth = gutter;
    this.viewWidth = Math.max(0, root.clientWidth - gutter);
    this.viewHeight = root.clientHeight;
    root.style.setProperty("--ce-gutter-width", `${gutter}px`);
    root.style.setProperty("--ce-text-width", `${this.viewWidth}px`);
    const radius = Math.max(0, (parseFloat(style.getPropertyValue("--penna-r")) || 0) - Math.max(1, parseFloat(style.borderTopWidth) || 0));
    const surfaceRadius = resolveRadius(radius, this.viewWidth, this.viewHeight);
    this.surface.style.borderRadius = gutter ? `0 ${surfaceRadius}px ${surfaceRadius}px 0` : `${surfaceRadius}px`;
    this.gutter.style.borderRadius = `${radius}px 0 0 ${radius}px`;
    const lineEls = this.linesEl.children;
    const numberEls = this.numbers.children;
    for (let i = 0; i < lineEls.length; i++) numberEls[i].style.height = `${lineEls[i].offsetHeight}px`;
    this.contentWidth = Math.max(...[...lineEls].map((line) => this.lineWidth(line)), 0);
    this.contentHeight = Math.max(this.linesEl.offsetHeight, this.lh);
    if (this.caretInView) this.follow = true;
    if (this.initialScroll) {
      this.scroll = { x: this.initialScroll[0], y: this.initialScroll[1] * this.lh };
      this.initialScroll = null;
    }
    this.paint();
  }

  lineWidth(line) {
    if (!line.firstChild) return 0;
    const range = document.createRange();
    range.selectNodeContents(line);
    const rects = [...range.getClientRects()];
    const left = line.getBoundingClientRect().left;
    return Math.max(0, ...rects.map((rect) => rect.right - left));
  }

  textPoint(line, local) {
    const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
    let node;
    let last = null;
    while ((node = walker.nextNode())) {
      if (local <= node.length) return [node, local];
      local -= node.length;
      last = node;
    }
    return last ? [last, last.length] : [line, 0];
  }

  position(offset) {
    const index = this.lineOf(offset);
    const line = this.linesEl.children[index];
    const local = offset - this.starts[index];
    if (!line) return { x: 0, y: 0, index };
    if (local === 0) return { x: 0, y: line.offsetTop, index };
    const [node, at] = this.textPoint(line, local);
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at);
    const rects = range.getClientRects();
    const rect = rects[rects.length - 1] ?? range.getBoundingClientRect();
    const box = line.getBoundingClientRect();
    const row = Math.max(0, Math.round((rect.top + rect.height / 2 - box.top - this.lh / 2) / this.lh));
    return { x: rect.left - box.left, y: line.offsetTop + row * this.lh, index };
  }

  quads(start, end) {
    const out = [];
    if (start >= end) return out;
    const first = this.lineOf(start);
    const last = this.lineOf(end);
    for (let index = first; index <= last; index++) {
      const lineStart = this.starts[index];
      const lineEnd = this.lineEnd(index);
      if (end < lineStart || start > lineEnd) continue;
      const line = this.linesEl.children[index];
      const from = Math.max(start, lineStart) - lineStart;
      const to = Math.min(end, lineEnd) - lineStart;
      const coversBreak = end > lineEnd;
      const box = line.getBoundingClientRect();
      const rows = new Map();
      if (to > from) {
        const range = document.createRange();
        range.setStart(...this.textPoint(line, from));
        range.setEnd(...this.textPoint(line, to));
        for (const rect of range.getClientRects()) {
          if (!rect.width) continue;
          const row = Math.max(0, Math.round((rect.top + rect.height / 2 - box.top - this.lh / 2) / this.lh));
          const left = rect.left - box.left;
          const right = rect.right - box.left;
          const seen = rows.get(row);
          rows.set(row, seen ? [Math.min(seen[0], left), Math.max(seen[1], right)] : [left, right]);
        }
      }
      if (coversBreak) {
        const tail = this.position(lineStart + to);
        const row = Math.round((tail.y - line.offsetTop) / this.lh);
        const seen = rows.get(row);
        rows.set(row, seen ? [seen[0], seen[1] + NEWLINE_SELECTION_WIDTH] : [tail.x, tail.x + NEWLINE_SELECTION_WIDTH]);
      }
      for (const [row, [left, right]] of rows) out.push({ x: left, y: line.offsetTop + row * this.lh, width: right - left });
    }
    return out;
  }

  clampScroll() {
    const overflowX = Math.max(0, this.contentWidth + CURSOR_WIDTH - this.viewWidth);
    const overflowY = Math.max(0, this.contentHeight - this.viewHeight);
    this.scroll.x = Math.min(0, Math.max(-overflowX, this.scroll.x));
    this.scroll.y = Math.min(0, Math.max(-overflowY, this.scroll.y));
  }

  paint() {
    if (this.lh === undefined) return;
    const caret = this.position(this.head);
    if (this.follow) {
      const top = caret.y;
      const bottom = top + this.lh;
      if (top + this.scroll.y < 0) this.scroll.y = -top;
      else if (bottom + this.scroll.y > this.viewHeight) this.scroll.y = this.viewHeight - bottom;
      if (caret.x + this.scroll.x < 0) this.scroll.x = -caret.x;
      else if (caret.x + CURSOR_WIDTH + this.scroll.x > this.viewWidth) this.scroll.x = this.viewWidth - caret.x - CURSOR_WIDTH;
      this.follow = false;
    }
    this.clampScroll();
    this.caretInView = caret.y + this.scroll.y >= 0 && caret.y + this.lh + this.scroll.y <= this.viewHeight && caret.x + this.scroll.x >= 0 && caret.x + CURSOR_WIDTH + this.scroll.x <= this.viewWidth;
    const root = this.root;
    root.style.setProperty("--ce-scroll-x", `${this.scroll.x}px`);
    root.style.setProperty("--ce-scroll-y", `${this.scroll.y}px`);
    const current = caret.index;
    const line = this.linesEl.children[current];
    this.active.style.top = `${line.offsetTop}px`;
    this.active.style.height = `${line.offsetHeight}px`;
    const relative = this.lineNumbers === "relative";
    [...this.numbers.children].forEach((number, index) => {
      number.textContent = relative && index !== current ? Math.abs(current - index) : index + 1;
      number.toggleAttribute("data-current", relative && index === current);
    });
    const [start, end] = this.highlightRange();
    this.quadLayer.replaceChildren(
      ...this.quads(start, end).map((quad) => {
        const element = document.createElement("div");
        element.className = "ce-quad";
        element.style.left = `${quad.x}px`;
        element.style.top = `${quad.y}px`;
        element.style.width = `${quad.width}px`;
        return element;
      }),
    );
    this.caret.style.left = `${caret.x}px`;
    this.caret.style.top = `${caret.y}px`;
    const block = this.vim && this.vim.mode !== "insert";
    this.caret.toggleAttribute("data-block", Boolean(block));
    const glyph = block ? this.text.slice(this.head, this.head + 1) : "";
    this.caret.textContent = glyph && glyph !== "\n" ? glyph : "";
    if (block && !this.caret.textContent) this.caret.style.width = "";
  }

  restartBlink() {
    this.caret.style.animation = "none";
    void this.caret.offsetWidth;
    this.caret.style.animation = "";
  }

  setSelection(anchor, head, keepGoal = false) {
    this.anchor = Math.max(0, Math.min(this.text.length, anchor));
    this.head = Math.max(0, Math.min(this.text.length, head));
    if (!keepGoal) this.goal = null;
    this.follow = true;
    this.paint();
    this.restartBlink();
  }

  move(head, extend, keepGoal = false) {
    this.setSelection(extend ? this.anchor : head, head, keepGoal);
  }

  edit(start, end, replacement, select = start + replacement.length) {
    this.history.push({ text: this.text, anchor: this.anchor, head: this.head });
    if (this.history.length > 200) this.history.shift();
    this.future = [];
    this.text = this.text.slice(0, start) + replacement + this.text.slice(end);
    this.anchor = this.head = select;
    this.follow = true;
    this.goal = null;
    this.render();
    this.restartBlink();
  }

  insert(value) {
    const [start, end] = this.range();
    this.edit(start, end, value);
  }

  undo(stack = this.history, other = this.future) {
    const entry = stack.pop();
    if (!entry) return;
    other.push({ text: this.text, anchor: this.anchor, head: this.head });
    Object.assign(this, entry);
    this.follow = true;
    this.render();
  }

  hit(clientX, clientY) {
    const box = this.linesEl.getBoundingClientRect();
    const y = Math.max(box.top + 0.5, Math.min(box.bottom - 0.5, clientY));
    if (clientY >= box.bottom) return this.text.length;
    const x = Math.max(box.left, clientX);
    const lines = [...this.linesEl.children];
    const line = lines.find((element) => element.getBoundingClientRect().bottom > y) ?? lines.at(-1);
    const index = Number(line.dataset.line);
    const start = this.starts[index];
    const end = this.lineEnd(index);
    if (start === end) return start;
    let best = start;
    let distance = Infinity;
    const lineBox = line.getBoundingClientRect();
    const row = Math.floor((y - lineBox.top) / this.lh);
    for (let offset = start; offset <= end; offset++) {
      const point = this.position(offset);
      if (Math.round((point.y - line.offsetTop) / this.lh) !== row) continue;
      const d = Math.abs(lineBox.left + point.x - x);
      if (d < distance) {
        distance = d;
        best = offset;
      }
    }
    return best;
  }

  onMouseDown(event) {
    if (event.button !== 0) return;
    event.preventDefault();
    if (this.disabled) return;
    this.root.focus();
    const offset = this.hit(event.clientX, event.clientY);
    if (this.vim && this.vim.mode === "visual") this.vim.mode = "normal";
    if (event.detail === 2) {
      let start = offset;
      let end = offset;
      while (start > 0 && isWord(this.text[start - 1])) start--;
      while (end < this.text.length && isWord(this.text[end])) end++;
      this.setSelection(start, end);
      return;
    }
    this.move(offset, event.shiftKey);
    const drag = (move) => {
      if (!(move.buttons & 1)) return;
      this.setSelection(this.anchor, this.hit(move.clientX, move.clientY));
    };
    const up = () => {
      removeEventListener("mousemove", drag);
      removeEventListener("mouseup", up);
    };
    addEventListener("mousemove", drag);
    addEventListener("mouseup", up);
  }

  onWheel(event) {
    if (this.lh === undefined) return;
    const before = { ...this.scroll };
    const dx = event.shiftKey && !event.deltaX ? event.deltaY : event.deltaX;
    const dy = event.shiftKey && !event.deltaX ? 0 : event.deltaY;
    this.scroll.x -= dx;
    this.scroll.y -= dy;
    this.clampScroll();
    if (before.x !== this.scroll.x || before.y !== this.scroll.y) {
      event.preventDefault();
      this.paint();
    }
  }

  wordLeft(offset) {
    while (offset > 0 && !isWord(this.text[offset - 1])) offset--;
    while (offset > 0 && isWord(this.text[offset - 1])) offset--;
    return offset;
  }

  wordRight(offset) {
    const length = this.text.length;
    while (offset < length && !isWord(this.text[offset])) offset++;
    while (offset < length && isWord(this.text[offset])) offset++;
    return offset;
  }

  vertical(offset, rows) {
    const point = this.position(offset);
    const goal = this.goal ?? point.x;
    const box = this.linesEl.getBoundingClientRect();
    const y = point.y + rows * this.lh;
    if (y < 0) return [0, goal];
    if (y >= this.contentHeight) return [this.text.length, goal];
    return [this.hit(box.left + goal, box.top + y + this.lh / 2), goal];
  }

  lineHome(offset) {
    const index = this.lineOf(offset);
    const start = this.starts[index];
    const indent = this.text.slice(start, this.lineEnd(index)).match(/^\s*/)[0].length;
    return offset === start + indent ? start : start + indent;
  }

  onKey(event) {
    if (this.disabled) return;
    const mod = event.metaKey || event.ctrlKey;
    if (this.vim && this.vim.mode !== "insert" && this.vimKey(event)) {
      event.preventDefault();
      return;
    }
    const extend = event.shiftKey;
    const [start, end] = this.range();
    const key = event.key;
    let handled = true;
    if (key === "ArrowLeft") {
      const target = event.altKey || (mod && !event.metaKey) ? this.wordLeft(this.head) : event.metaKey ? this.lineHome(this.head) : !extend && start !== end ? start : this.head - 1;
      this.move(target, extend);
    } else if (key === "ArrowRight") {
      const target = event.altKey || (mod && !event.metaKey) ? this.wordRight(this.head) : event.metaKey ? this.lineEnd(this.lineOf(this.head)) : !extend && start !== end ? end : this.head + 1;
      this.move(target, extend);
    } else if (key === "ArrowUp" || key === "ArrowDown") {
      if (mod) this.move(key === "ArrowUp" ? 0 : this.text.length, extend);
      else {
        const [target, goal] = this.vertical(this.head, key === "ArrowUp" ? -1 : 1);
        this.move(target, extend, true);
        this.goal = goal;
      }
    } else if (key === "PageUp" || key === "PageDown") {
      const rows = Math.max(1, Math.floor(this.viewHeight / this.lh));
      const [target, goal] = this.vertical(this.head, key === "PageUp" ? -rows : rows);
      this.move(target, extend, true);
      this.goal = goal;
    } else if (key === "Home") {
      this.move(mod ? 0 : this.lineHome(this.head), extend);
    } else if (key === "End") {
      this.move(mod ? this.text.length : this.lineEnd(this.lineOf(this.head)), extend);
    } else if (mod && key.toLowerCase() === "a") {
      this.setSelection(0, this.text.length);
    } else if (mod && key.toLowerCase() === "z") {
      if (event.shiftKey) this.undo(this.future, this.history);
      else this.undo();
    } else if (mod && key.toLowerCase() === "y") {
      this.undo(this.future, this.history);
    } else if (mod) {
      handled = false;
    } else if (key === "Backspace") {
      if (start !== end) this.insert("");
      else if (start > 0) {
        const index = this.lineOf(start);
        const before = this.text.slice(this.starts[index], start);
        const width = /^ +$/.test(before) ? ((before.length - 1) % TAB) + 1 : 1;
        this.edit(start - (event.altKey ? start - this.wordLeft(start) : width), start, "");
      }
    } else if (key === "Delete") {
      if (start !== end) this.insert("");
      else if (start < this.text.length) this.edit(start, event.altKey ? this.wordRight(start) : start + 1, "");
    } else if (key === "Enter") {
      const index = this.lineOf(start);
      const indent = this.text.slice(this.starts[index], start).match(/^[ \t]*/)[0];
      const opens = /[{([]\s*$/.test(this.text.slice(this.starts[index], start));
      this.insert(`\n${indent}${opens ? " ".repeat(TAB) : ""}`);
    } else if (key === "Tab") {
      if (start !== end && this.lineOf(start) !== this.lineOf(end)) this.indentLines(event.shiftKey ? -1 : 1);
      else if (event.shiftKey) this.indentLines(-1);
      else {
        const column = start - this.starts[this.lineOf(start)];
        this.insert(" ".repeat(TAB - (column % TAB)));
      }
    } else if (key === "Escape") {
      if (this.vim) {
        this.vim.mode = "normal";
        this.move(Math.max(this.starts[this.lineOf(this.head)], this.head - 1), false);
      } else this.move(this.head, false);
    } else if (key.length === 1 && !event.altKey) {
      this.insert(key);
    } else {
      handled = false;
    }
    if (handled) event.preventDefault();
  }

  indentLines(direction) {
    const [start, end] = this.range();
    const first = this.lineOf(start);
    const last = this.lineOf(Math.max(start, end - (end > start && this.text[end - 1] === "\n" ? 1 : 0)));
    const from = this.starts[first];
    const to = this.lineEnd(last);
    const lines = this.text.slice(from, to).split("\n");
    const changed = lines.map((line) => (direction > 0 ? " ".repeat(TAB) + line : line.replace(new RegExp(`^ {1,${TAB}}`), ""))).join("\n");
    this.edit(from, to, changed, from + changed.length);
    this.anchor = from;
    this.paint();
  }

  vimKey(event) {
    const vim = this.vim;
    const key = event.key;
    if (event.metaKey || event.ctrlKey || key.length > 1 && !["Escape", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(key)) return false;
    const index = this.lineOf(this.head);
    const lineStart = this.starts[index];
    const lineEnd = this.lineEnd(index);
    const lastChar = Math.max(lineStart, lineEnd - (vim.mode === "visual" ? 0 : 1));
    const go = (offset, keepGoal = false) => {
      this.anchor = offset;
      this.setSelection(offset, offset, keepGoal);
    };
    const motions = {
      h: () => Math.max(lineStart, this.head - 1),
      ArrowLeft: () => Math.max(lineStart, this.head - 1),
      l: () => Math.min(lastChar, this.head + 1),
      ArrowRight: () => Math.min(lastChar, this.head + 1),
      0: () => lineStart,
      $: () => lastChar,
      w: () => {
        let offset = this.head;
        const length = this.text.length;
        if (isWord(this.text[offset])) while (offset < length && isWord(this.text[offset])) offset++;
        else if (offset < length && !/\s/.test(this.text[offset])) offset++;
        while (offset < length && /\s/.test(this.text[offset])) offset++;
        return Math.min(offset, Math.max(0, length - 1));
      },
      b: () => this.wordLeft(this.head),
      G: () => this.starts.at(-1),
    };
    const verticals = { j: 1, k: -1, ArrowDown: 1, ArrowUp: -1 };
    if (key === "Escape") {
      vim.mode = "normal";
      go(Math.min(this.head, lastChar));
      return true;
    }
    if (key in motions || key in verticals) {
      let target;
      let keepGoal = false;
      if (key in verticals) {
        const [offset, goal] = this.vertical(this.head, verticals[key]);
        const row = this.lineOf(offset);
        target = Math.min(offset, Math.max(this.starts[row], this.lineEnd(row) - (vim.mode === "visual" ? 0 : 1)));
        keepGoal = true;
        this.goal = goal;
      } else target = motions[key]();
      if (vim.mode === "visual") this.setSelection(target, target, keepGoal);
      else go(target, keepGoal);
      return true;
    }
    if (vim.mode === "visual") {
      const [start, end] = this.highlightRange();
      if (key === "v") {
        vim.mode = "normal";
        go(this.head);
      } else if (key === "d" || key === "x") {
        vim.mode = "normal";
        this.edit(start, end, "", start);
      } else if (key === "y") {
        navigator.clipboard?.writeText(this.text.slice(start, end)).catch(() => {});
        vim.mode = "normal";
        go(start);
      } else if (key === "o") {
        const anchor = vim.anchor;
        vim.anchor = this.head;
        this.setSelection(anchor, anchor);
      }
      return true;
    }
    if (key === "v") {
      vim.mode = "visual";
      vim.anchor = this.head;
      this.paint();
    } else if (key === "i") {
      vim.mode = "insert";
      go(this.head);
    } else if (key === "a") {
      vim.mode = "insert";
      go(Math.min(lineEnd, this.head + 1));
    } else if (key === "I") {
      vim.mode = "insert";
      go(this.lineHome(lineStart) === lineStart ? lineStart : this.lineHome(lineStart));
    } else if (key === "A") {
      vim.mode = "insert";
      go(lineEnd);
    } else if (key === "o" || key === "O") {
      vim.mode = "insert";
      const indent = this.text.slice(lineStart, lineEnd).match(/^[ \t]*/)[0];
      if (key === "o") this.edit(lineEnd, lineEnd, `\n${indent}`);
      else this.edit(lineStart, lineStart, `${indent}\n`, lineStart + indent.length);
    } else if (key === "x") {
      if (lineEnd > lineStart) this.edit(this.head, this.head + 1, "", Math.min(this.head, Math.max(lineStart, lineEnd - 2)));
    } else if (key === "u") {
      this.undo();
    }
    return true;
  }
}

function mount(root = document) {
  root.querySelectorAll(".penna-code-editor").forEach((element) => {
    if (!element.pennaCodeEditor) element.pennaCodeEditor = new CodeEditor(element);
  });
}

function relayout() {
  document.querySelectorAll(".penna-code-editor").forEach((element) => {
    const editor = element.pennaCodeEditor;
    if (!editor) return;
    editor.ready = true;
    editor.layout();
  });
}

mount();
document.querySelectorAll(".penna-code-editor[autofocus]").forEach((element) => element.focus({ preventScroll: true }));
document.fonts.ready.then(relayout);
addEventListener("load", relayout);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && relayout(), true);

window.pennaCodeEditor = { mount, CodeEditor };
})();
