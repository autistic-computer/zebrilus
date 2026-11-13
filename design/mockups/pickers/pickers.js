const WEIGHTS = {
  match: 10,
  wordStart: 18,
  consecutive: 14,
  base: 10,
  gap: 1,
  window: 24,
  session: 16,
  host: 8,
  git: 12,
  recent: 20,
  depth: 3,
  floor: 0.6,
};

const LIMIT = 50;
const FILE_LIMIT = 5;
const SEPARATORS = " /_-.›·";
const CRUMB_BREAKS = " /›·";

function fuzzy(query, text, baseFrom = 0) {
  const needle = query.replace(/\s+/g, "");
  if (!needle) return { score: 0, hits: [] };
  const smart = needle !== needle.toLowerCase();
  const q = smart ? needle : needle.toLowerCase();
  const s = smart ? text : text.toLowerCase();
  const n = s.length;
  const m = q.length;
  if (m > n) return null;
  const bonus = [];
  const starts = [];
  for (let j = 0; j < n; j++) {
    const before = text[j - 1];
    const start = j === 0 || SEPARATORS.includes(before) || (/[a-z]/.test(before) && /[A-Z]/.test(text[j]));
    starts.push(start);
    bonus.push(WEIGHTS.match + (start ? WEIGHTS.wordStart : 0) + (j >= baseFrom ? WEIGHTS.base : 0));
  }
  const scores = [];
  const links = [];
  for (let i = 0; i < m; i++) {
    const row = new Array(n).fill(-Infinity);
    const link = new Array(n).fill(-1);
    let run = -Infinity;
    let runAt = -1;
    for (let j = i; j < n; j++) {
      if (i > 0 && j >= 2) {
        run -= WEIGHTS.gap;
        const reach = scores[i - 1][j - 2] - WEIGHTS.gap;
        if (reach > run) {
          run = reach;
          runAt = j - 2;
        }
      }
      if (s[j] !== q[i]) continue;
      if (i === 0) {
        if (starts[j]) row[j] = bonus[j];
        continue;
      }
      const joined = scores[i - 1][j - 1] + WEIGHTS.consecutive;
      if (joined > -Infinity && (joined >= run || !starts[j])) {
        row[j] = bonus[j] + joined;
        link[j] = j - 1;
      } else if (run > -Infinity && starts[j]) {
        row[j] = bonus[j] + run;
        link[j] = runAt;
      }
    }
    scores.push(row);
    links.push(link);
  }
  let end = -1;
  scores[m - 1].forEach((value, j) => {
    if (value > -Infinity && (end < 0 || value > scores[m - 1][end])) end = j;
  });
  if (end < 0) return null;
  const hits = [];
  for (let i = m - 1, j = end; i >= 0; i--) {
    hits.unshift(j);
    j = links[i][j];
  }
  return { score: scores[m - 1][end], hits };
}

let serial = 0;

function makeNode(kind, props, children = null) {
  const node = { id: `${kind}-${serial++}`, kind, ...props, children };
  children?.forEach((child) => (child.parent = node));
  return node;
}

const host = (name, status, sessions) => makeNode("host", { name, status }, sessions);
const session = (name, windows, recent = 0) => makeNode("session", { name, recent }, windows);
const win = (index, name, panes, recent = 0) => makeNode("window", { index, name, recent }, panes);
const pane = (program, title, recent = 0) => makeNode("pane", { name: program, title, recent });
const chat = (title, agent) => makeNode("chat", { name: title, agent });
const action = (name, arg = null) => makeNode("action", { name, arg });

const PLACE_DEPTH = { host: 0, session: 1, window: 2, pane: 3, chat: 3 };
const ICONS = {
  host: "hard-drive",
  session: "layers",
  window: "panels-top-left",
  pane: "square-terminal",
  chat: "chat",
  dir: "folder",
  file: "file",
  action: "bolt",
  slot: "pencil",
};

function stamp(node, place = {}) {
  node.place = { ...place, [node.kind]: node };
  node.children?.forEach((child) => stamp(child, node.place));
}

function placeAddress(node) {
  const parts = [];
  for (let at = node; at; at = at.parent) {
    if (at.kind === "host" || at.kind === "session") parts.unshift(at.name);
    else if (at.kind === "window") parts.unshift(String(at.index));
    else parts.unshift(String(at.parent.children.indexOf(at) + 1));
  }
  return parts.join(" › ");
}

function hostOf(node) {
  return node.place?.host?.name ?? "";
}

const PROJECT = "dev/autistic-computer/zebrilus";

const PROJECT_FILES = [
  "AGENTS.md",
  "CLAUDE.md",
  "Cargo.lock",
  "Cargo.toml",
  ["Justfile", "", 0.4],
  "LICENSE-APACHE",
  "LICENSE-MIT",
  "NOTICE.md",
  "README.md",
  "SECURITY.md",
  "install.sh",
  "mise.toml",
  "rust-toolchain.toml",
  "crates/zz/Cargo.toml",
  "crates/zz/src/main.rs",
  "crates/zz/src/cli.rs",
  "crates/zz-agent/Cargo.toml",
  "crates/zz-agent/src/lib.rs",
  "crates/zz-agent/src/chat.rs",
  "crates/zz-agent/src/resume.rs",
  "crates/zz-daemon/Cargo.toml",
  "crates/zz-daemon/src/lib.rs",
  "crates/zz-daemon/src/server.rs",
  "crates/zz-daemon/src/session.rs",
  "crates/zz-daemon/tests/attach.rs",
  "crates/zz-term/Cargo.toml",
  "crates/zz-term/src/lib.rs",
  "crates/zz-term/src/grid.rs",
  "crates/zz-term/src/parser.rs",
  "crates/zz-ui/Cargo.toml",
  "crates/zz-ui/src/lib.rs",
  ["crates/zz-ui/src/target.rs", "M", 0.9],
  ["crates/zz-ui/src/palette.rs", "M", 0.6],
  ["crates/zz-ui/src/mention.rs", "A", 0.5],
  "crates/zz-ui/src/file_picker.rs",
  "crates/zz-ui/src/composer.rs",
  "crates/zz-ui/src/pane_header.rs",
  "crates/zz-ui/src/theme.rs",
  "crates/zz-ui/examples/theme_css.rs",
  "design/README.md",
  "design/index.html",
  ["design/zz.css", "M", 0.3],
  "design/mockups/pickers/index.html",
  "design/mockups/pickers/pickers.js",
  "docs/keys.md",
  "docs/targets.md",
  "knowledge/pickers.md",
  "knowledge/targeting.md",
  "libs/penna/Cargo.toml",
  "libs/penna/src/lib.rs",
  "libs/penna/src/blocks/palette/mod.rs",
  "libs/penna/src/blocks/palette/palette.css",
  "scripts/bench.sh",
  "scripts/release.sh",
];

const HOME_FILES = [
  ".config/zz/config.toml",
  ".config/zz/keys.toml",
  "dev/autistic-computer/ala/README.md",
  "dev/autistic-computer/nidus/Cargo.toml",
  "dev/autistic-computer/nidus/README.md",
  "notes/pickers.md",
  "notes/todo.md",
  ...PROJECT_FILES.map((entry) => (Array.isArray(entry) ? [`${PROJECT}/${entry[0]}`, ...entry.slice(1)] : `${PROJECT}/${entry}`)),
];

function buildTree(entries) {
  const root = makeNode("dir", { name: "~", path: "" }, []);
  for (const entry of entries) {
    const [path, git = "", recent = 0] = Array.isArray(entry) ? entry : [entry];
    const names = path.split("/");
    let dir = root;
    names.forEach((name, index) => {
      const at = names.slice(0, index + 1).join("/");
      if (index === names.length - 1) {
        const file = makeNode("file", { name, path: at, git, recent });
        file.parent = dir;
        dir.children.push(file);
        return;
      }
      let next = dir.children.find((child) => child.kind === "dir" && child.name === name);
      if (!next) {
        next = makeNode("dir", { name, path: at }, []);
        next.parent = dir;
        dir.children.push(next);
      }
      dir = next;
    });
  }
  return root;
}

function findDir(root, path) {
  return path.split("/").reduce((dir, name) => dir.children.find((child) => child.kind === "dir" && child.name === name), root);
}

function findFile(root, path) {
  const names = path.split("/");
  const dir = findDir(root, names.slice(0, -1).join("/"));
  return dir.children.find((child) => child.name === names.at(-1));
}

const HOME = buildTree(HOME_FILES);
const FOLDER = findDir(HOME, PROJECT);

const NVIM = pane("nvim", "crates/zz-ui/src/target.rs");
const COMPOSER_CHAT = chat("Pickers mockup", "claude");
const RECENT_PANE = pane("zsh", "cargo build --release", 0.5);
const RECENT_WINDOW = win(2, "tests", [pane("zsh", "cargo nextest run"), pane("nvim", "crates/zz-daemon/tests/attach.rs")], 0.8);
const RECENT_SESSION = session(
  "penna",
  [
    win(1, "docs", [pane("nvim", "docs/pages/palette.md"), pane("zsh", "just docs serve")]),
    win(2, "build", [pane("zsh", "cargo build -p penna")]),
  ],
  0.6,
);

const HERE_HOST = host("mbp", "online", [
  session("zebrilus", [
    win(1, "editor", [NVIM, pane("zsh", "cargo test -p zz-ui"), COMPOSER_CHAT]),
    win(2, "agents", [chat("Tab key model for pickers", "claude"), pane("zsh", "just design serve")]),
    win(3, "logs", [pane("tail", "logs/zz.log")]),
  ]),
  RECENT_SESSION,
  session("dotfiles", [win(1, "main", [pane("nvim", ".bashrc")])]),
]);

const OTHER_HOSTS = [
  host("wsl-archlinux", "online", [
    session("build", [win(1, "cargo", [RECENT_PANE]), win(2, "bench", [pane("zsh", "just bench")])]),
    session("pkgs", [win(1, "makepkg", [pane("zsh", "makepkg -si")])]),
  ]),
  host("xps", "online", [
    session("work", [win(1, "shell", [chat("Fix daemon reattach race", "codex"), pane("zsh", "htop")]), RECENT_WINDOW]),
    session("notes", [win(1, "main", [pane("nvim", "todo.md")])]),
  ]),
];

const HOSTS = [HERE_HOST, ...OTHER_HOSTS];
HOSTS.forEach((each) => stamp(each));

const ACTIONS = [
  action("split right"),
  action("split down"),
  action("new window"),
  action("rename session", "text"),
  action("kill pane"),
  action("insert path", "path"),
];

const DELETED = makeNode("file", { name: "at_menu.rs", path: `${PROJECT}/crates/zz-ui/src/at_menu.rs`, git: "D", recent: 0 });
const CHANGED = [
  findFile(HOME, `${PROJECT}/crates/zz-ui/src/target.rs`),
  findFile(HOME, `${PROJECT}/crates/zz-ui/src/palette.rs`),
  findFile(HOME, `${PROJECT}/crates/zz-ui/src/mention.rs`),
  DELETED,
];

function context(paneNode) {
  const window = paneNode?.parent ?? null;
  const sessionNode = window?.parent ?? null;
  return { pane: paneNode, window, session: sessionNode, host: sessionNode?.parent ?? null, folder: FOLDER };
}

const fileish = (node) => node.kind === "file" || node.kind === "dir";
const isPlace = (node) => node.kind in PLACE_DEPTH;
const count = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;

function descendants(node, keep = () => true) {
  const out = [];
  const walk = (at) =>
    at.children?.forEach((child) => {
      if (keep(child)) out.push(child);
      walk(child);
    });
  walk(node);
  return out;
}

function sortDir(children) {
  return [...children].sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "dir" ? -1 : 1));
}

function childrenOf(node) {
  return node.kind === "dir" ? sortDir(node.children) : (node.children ?? []);
}

function relPath(node, base) {
  if (!base || base.kind !== "dir") return node.path;
  return base.path ? node.path.slice(base.path.length + 1) : node.path;
}

function fullPath(node) {
  return `~/${node.path}${node.kind === "dir" && node.path ? "/" : ""}`;
}

function placeDetail(node) {
  const parts = [];
  for (let at = node.parent; at; at = at.parent) parts.unshift(at.kind === "window" ? String(at.index) : at.name);
  return parts.join(" › ");
}

function describe(node, flat, base) {
  const row = { node, icon: ICONS[node.kind], pre: "", name: node.name, sub: "", detail: "", right: "", dot: "", git: "" };
  row.crumb = node.name;
  if (isPlace(node)) row.address = placeAddress(node);
  if (fileish(node)) row.address = fullPath(node);
  switch (node.kind) {
    case "host":
      row.right = count(node.children.length, "session");
      row.dot = node.status;
      row.inside = true;
      break;
    case "session":
      row.right = count(node.children.length, "window");
      row.inside = true;
      if (flat) row.detail = placeDetail(node);
      break;
    case "window":
      row.pre = String(node.index);
      row.right = count(node.children.length, "pane");
      row.inside = true;
      if (flat) row.detail = placeDetail(node);
      break;
    case "pane":
      row.sub = node.title;
      if (flat) row.detail = placeDetail(node);
      break;
    case "chat":
      row.detail = `${node.agent} · ${hostOf(node)}`;
      break;
    case "dir":
      row.inside = true;
      if (flat) row.detail = relPath(node, base).slice(0, -node.name.length);
      break;
    case "file":
      row.git = node.git;
      if (flat) row.detail = relPath(node, base).slice(0, -node.name.length);
      break;
    case "action":
      row.inside = Boolean(node.arg);
      row.argument = Boolean(node.arg);
      break;
  }
  return row;
}

function closeness(node, ctx) {
  if (node.kind === "action") return WEIGHTS.window;
  const place = node.place;
  if (!place || !ctx.host) return 0;
  if (place.window && place.window === ctx.window) return WEIGHTS.window;
  if (place.session && place.session === ctx.session) return WEIGHTS.session;
  if (place.host === ctx.host) return WEIGHTS.host;
  return 0;
}

function depth(node, base) {
  if (fileish(node)) return relPath(node, base).split("/").length - 1;
  if (!isPlace(node)) return 0;
  const from = base && isPlace(base) ? PLACE_DEPTH[base.kind] + 1 : 0;
  return Math.max(PLACE_DEPTH[node.kind] - from, 0);
}

function prior(node, ctx, base) {
  return closeness(node, ctx) + (node.git ? WEIGHTS.git : 0) + WEIGHTS.recent * (node.recent ?? 0) - WEIGHTS.depth * depth(node, base);
}

function contiguous(token, text) {
  const smart = token !== token.toLowerCase();
  const hay = smart ? text : text.toLowerCase();
  const needle = smart ? token : token.toLowerCase();
  let best = null;
  for (let at = hay.indexOf(needle); at >= 0; at = hay.indexOf(needle, at + 1)) {
    if ([...text.slice(at, at + needle.length)].some((c) => CRUMB_BREAKS.includes(c))) continue;
    const start = at === 0 || SEPARATORS.includes(text[at - 1]);
    const score = WEIGHTS.match * needle.length + WEIGHTS.consecutive * (needle.length - 1) + (start ? WEIGHTS.wordStart : 0);
    if (!best || score > best.score) best = { score, hits: Array.from(needle, (_, i) => at + i) };
  }
  return best;
}

function matchRow(query, row) {
  const label = row.sub ? `${row.name} ${row.sub}` : row.name;
  const subFrom = row.name.length + 1;
  const split = (labelHits, detailHits) => ({
    detail: detailHits,
    name: labelHits.filter((at) => at < row.name.length),
    sub: labelHits.filter((at) => at >= subFrom).map((at) => at - subFrom),
  });
  const file = fileish(row.node);
  if (file && query.includes("/")) {
    const cut = row.detail.length;
    const match = fuzzy(query, row.detail + label, cut);
    if (!match) return null;
    return { score: match.score, hits: split(match.hits.filter((at) => at >= cut).map((at) => at - cut), match.hits.filter((at) => at < cut)) };
  }
  let score = 0;
  const labelHits = [];
  const detailHits = [];
  for (const token of query.split(file ? /\s+/ : /[\s/]+/).filter(Boolean)) {
    const inLabel = fuzzy(token, label, 0);
    const inDetail = token.length > 1 && row.detail ? contiguous(token, row.detail) : null;
    if (!inLabel && !inDetail) return null;
    const useLabel = inLabel && (!inDetail || inLabel.score >= inDetail.score);
    const pick = useLabel ? inLabel : inDetail;
    score += pick.score;
    (useLabel ? labelHits : detailHits).push(...pick.hits);
  }
  return { score, hits: split(labelHits, detailHits) };
}

function rank(nodes, query, ctx, base) {
  const out = [];
  for (const node of nodes) {
    const row = describe(node, true, base);
    const match = matchRow(query, row);
    if (!match) continue;
    row.hits = match.hits;
    row.score = match.score + prior(node, ctx, base);
    row.length = row.detail.length + row.name.length + row.sub.length;
    out.push(row);
  }
  out.sort((a, b) => b.score - a.score || a.length - b.length || a.name.localeCompare(b.name));
  const floor = out.length && out[0].score > 0 ? out[0].score * WEIGHTS.floor : -Infinity;
  return out.filter((row) => row.score >= floor);
}

const header = (label) => ({ header: label });
const keyed = (rows, section) => rows.map((row) => ({ ...row, key: `${section}:${row.node.id}` }));

function slotRow(actionNode, query, ctx) {
  const node = { id: "slot", kind: "slot" };
  return { node, key: "slot", icon: ICONS.slot, pre: "", name: query, sub: query ? "" : (ctx.session?.name ?? ""), detail: "", right: "", off: !query };
}

function levelList(picker, ctx) {
  const node = picker.level.node;
  const query = picker.query;
  if (node.kind === "action" && node.arg === "text") return [slotRow(node, query, ctx)];
  const at = node.kind === "action" ? ctx.folder : node;
  if (!query) return keyed(childrenOf(at).map((child) => describe(child, false, at)), "c");
  const pool = at.kind === "dir" ? descendants(at) : descendants(at, isPlace);
  return keyed(rank(pool, query, ctx, at).slice(0, LIMIT), "r");
}

function fileLevel(picker) {
  const node = picker.level.node;
  return node.kind === "dir" || (node.kind === "action" && node.arg === "path");
}

function argumentOf(picker) {
  return picker.path.find((frame) => frame.node.kind === "action")?.node ?? null;
}

function allPlaces() {
  return HOSTS.flatMap((each) => [each, ...descendants(each, isPlace)]);
}

function paletteSource() {
  const ctx = context(NVIM);
  const root = { id: "palette-root", kind: "root" };
  return {
    name: "palette",
    ctx,
    verbs: ["go", "run", "open"],
    start: () => [{ node: root, crumb: null }],
    fileLevel,
    list(picker) {
      if (picker.level.node !== root) return levelList(picker, ctx);
      const query = picker.query;
      if (!query) {
        const recent = allPlaces()
          .filter((each) => each.recent)
          .sort((a, b) => b.recent - a.recent);
        return [
          header("Recent"),
          ...keyed(recent.map((each) => describe(each, true)), "recent"),
          header("Sessions on this host"),
          ...keyed(ctx.host.children.map((each) => describe(each, false)), "sessions"),
          header("Hosts"),
          ...keyed(OTHER_HOSTS.map((each) => describe(each, false)), "hosts"),
        ];
      }
      let ranked = rank([...ACTIONS, ...allPlaces(), ...descendants(ctx.folder)], query, ctx, ctx.folder);
      if (!/[/.]/.test(query)) {
        let shown = 0;
        ranked = ranked.filter((row) => !fileish(row.node) || ++shown <= FILE_LIMIT);
      }
      return keyed(ranked.slice(0, LIMIT), "r");
    },
    verb(row, picker) {
      if (argumentOf(picker)) return "run";
      if (!row) return "go";
      return { action: "run", file: "open", dir: "open" }[row.node.kind] ?? "go";
    },
    take(row, picker) {
      const arg = argumentOf(picker);
      if (arg) return `run → ${arg.name} ${row.node.kind === "slot" ? picker.query : fullPath(row.node)}`;
      if (row.node.kind === "action") return `run → ${row.name}`;
      return `${this.verb(row, picker)} → ${row.address}`;
    },
  };
}

function mentionText(node, ctx) {
  if (!fileish(node)) return placeAddress(node);
  const base = ctx.folder.path;
  const tail = node.kind === "dir" ? "/" : "";
  if (node.path === base) return "./";
  if (node.path.startsWith(`${base}/`)) return node.path.slice(base.length + 1) + tail;
  return fullPath(node);
}

function mentionSource() {
  const ctx = context(COMPOSER_CHAT);
  const root = { id: "mention-root", kind: "root" };
  return {
    name: "mention",
    ctx,
    verbs: ["mention"],
    start: () => [{ node: root, crumb: null }],
    fileLevel,
    list(picker) {
      if (picker.level.node !== root) return levelList(picker, ctx);
      const query = picker.query;
      if (!query) {
        const here = ctx.window.children.filter((each) => each !== ctx.pane);
        const chats = allPlaces().filter((each) => each.kind === "chat" && each !== ctx.pane);
        return [
          header("Here"),
          ...keyed(here.map((each) => describe(each, false)), "here"),
          header("Changed"),
          ...keyed(CHANGED.map((each) => describe(each, true, ctx.folder)), "changed"),
          header("Chats"),
          ...keyed(chats.map((each) => describe(each, false)), "chats"),
          header("Places"),
          ...keyed([ctx.session, ...OTHER_HOSTS].map((each) => describe(each, false)), "places"),
          { rule: true },
          { ...describe(ctx.folder, false), key: "browse:here", name: "./", detail: ctx.folder.name, crumb: "./" },
          { ...describe(HOME, false), key: "browse:home", name: "~/", crumb: "~" },
        ];
      }
      const pool = [...allPlaces().filter((each) => each !== ctx.pane), ...descendants(ctx.folder)];
      return keyed(rank(pool, query, ctx, ctx.folder).slice(0, LIMIT), "r");
    },
    verb: () => "mention",
    take: (row) => mentionText(row.node, ctx),
  };
}

function fileSource() {
  const ctx = { pane: null, window: null, session: null, host: null, folder: FOLDER };
  const trail = [];
  for (let at = FOLDER; at; at = at.parent) trail.unshift(at);
  return {
    name: "files",
    ctx,
    verbs: ["insert"],
    start: () => trail.map((node) => ({ node, crumb: node.name })),
    fileLevel,
    list: (picker) => levelList(picker, ctx),
    verb: () => "insert",
    take: (row) => `insert → ${fullPath(row.node)}`,
  };
}

class Picker {
  constructor(source) {
    this.source = source;
    this.tabMode = "nothing";
    this.reset();
  }

  reset() {
    const start = this.source.start();
    this.path = start.map((frame, index) => ({ ...frame, query: "", key: start[index + 1] ? `c:${start[index + 1].node.id}` : null }));
    this.query = "";
    this.refresh();
  }

  get level() {
    return this.path.at(-1);
  }

  get row() {
    return this.rows[this.sel] ?? null;
  }

  get crumbs() {
    return this.path.map((frame) => frame.crumb).filter(Boolean);
  }

  refresh(key = null) {
    this.entries = this.source.list(this);
    this.rows = this.entries.filter((entry) => entry.node);
    const at = key ? this.rows.findIndex((row) => row.key === key) : -1;
    this.sel = Math.max(at, 0);
  }

  select(key) {
    const at = this.rows.findIndex((row) => row.key === key);
    if (at >= 0) this.sel = at;
  }

  move(step) {
    if (!this.rows.length) return null;
    this.sel = (this.sel + step + this.rows.length) % this.rows.length;
    return null;
  }

  type(query) {
    let went = null;
    while (this.source.fileLevel(this) && query.indexOf("/") > 0) {
      const cut = query.indexOf("/");
      const name = query.slice(0, cut);
      this.query = name;
      this.refresh();
      const at = this.rows.findIndex((row) => row.node.kind === "dir" && row.node.name === name);
      if (at < 0) break;
      this.sel = at;
      went = this.into();
      query = query.slice(cut + 1);
    }
    this.query = query;
    this.refresh();
    return went;
  }

  into() {
    const row = this.row;
    if (!row?.inside) return null;
    this.level.query = this.query;
    this.level.key = row.key;
    this.path.push({ node: row.node, crumb: row.crumb, query: "", key: null });
    this.query = "";
    this.refresh();
    return { kind: "into" };
  }

  back() {
    if (this.path.length < 2) return null;
    this.path.pop();
    this.query = this.level.query;
    this.refresh(this.level.key);
    return { kind: "back" };
  }

  take() {
    const row = this.row;
    if (!row || row.off) return null;
    if (row.argument) return this.into();
    return { kind: "take", row, text: this.source.take(row, this) };
  }

  copy() {
    const row = this.row;
    return row?.address ? { kind: "copy", row, text: row.address } : null;
  }

  tab() {
    const row = this.row;
    if (row?.inside) return this.into();
    if (row && this.tabMode === "take") return this.take();
    return null;
  }

  hints() {
    const row = this.row;
    const verb = this.source.verb(row, this);
    const leaf = this.tabMode === "take" && row && !row.inside;
    return [
      { keys: "enter", words: this.source.verbs, word: verb, on: Boolean(row && !row.off) },
      { keys: "alt-enter", words: ["copy"], word: "copy", on: Boolean(row?.address) },
      { keys: "tab", words: ["into", ...this.source.verbs], word: leaf ? verb : "into", on: Boolean(row?.inside || (leaf && !row.off)) },
      { keys: "shift-tab", words: ["back"], word: "back", on: this.path.length > 1 },
    ];
  }
}

const sources = { palette: paletteSource, mention: mentionSource, files: fileSource };

function mentionStart(value, caret) {
  let at = caret - 1;
  while (at >= 0 && value[at] !== "@" && !/\s/.test(value[at])) at--;
  return at >= 0 && value[at] === "@" && (at === 0 || /\s/.test(value[at - 1])) ? at : -1;
}

function cutQuery(value, at, length) {
  return { before: value.slice(0, at), after: value.slice(at + 1 + length) };
}

function swapQuery(value, at, length, query) {
  const { before, after } = cutQuery(value, at, length);
  return `${before}@${query}${after}`;
}

if (typeof module === "object") module.exports = { WEIGHTS, fuzzy, Picker, sources, mentionStart, cutQuery, swapQuery };

function esc(text) {
  return String(text).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

function marked(text, hits = []) {
  if (!hits.length) return esc(text);
  const on = new Set(hits);
  let out = "";
  let open = false;
  for (let i = 0; i < text.length; i++) {
    if (on.has(i) !== open) {
      open = !open;
      out += open ? `<span class="penna-palette-match">` : "</span>";
    }
    out += esc(text[i]);
  }
  return out + (open ? "</span>" : "");
}

function rowHtml(row, selected) {
  const hits = row.hits ?? {};
  const pre = row.pre ? `<span class="penna-palette-muted">${esc(row.pre)}&nbsp;&nbsp;</span>` : "";
  const sub = row.sub ? `<span class="penna-palette-muted">${row.name ? "&nbsp;&nbsp;" : ""}${marked(row.sub, hits.sub)}</span>` : "";
  const shown = row.detail.length > 1 && row.detail.endsWith("/") ? row.detail.slice(0, -1) : row.detail;
  const detail = shown ? `<span class="penna-palette-detail">${marked(shown, hits.detail)}</span>` : "";
  const meta = [
    row.git ? `<span class="penna-palette-right zz-git" data-git="${row.git}">${row.git}</span>` : "",
    row.right ? `<span class="penna-palette-right">${esc(row.right)}</span>` : "",
    row.dot ? `<span class="penna-palette-dot" data-status="${row.dot}"></span>` : "",
  ].join("");
  const state = selected ? ` data-state="selected" aria-selected="true"` : "";
  return `<div class="penna-list-item penna-palette-row" data-penna="menu-item" role="option" data-key="${esc(row.key)}"${detail ? " data-detail" : ""}${state}><div class="penna-palette-content"><i class="penna-palette-icon" data-icon="${row.icon}"></i><span class="penna-palette-label">${pre}${marked(row.name, hits.name)}${sub}</span>${detail}${meta}<span class="zz-inside">${row.inside ? `<i data-icon="chevron-right"></i>` : ""}</span></div></div>`;
}

function crumbHtml(label) {
  return `<span class="penna-badge penna-palette-pill" data-penna="badge" data-kind="crumb"><span>${esc(label)}</span></span>`;
}

function fitCrumbs(pills, lead, crumbs) {
  pills.innerHTML = lead + crumbs.map(crumbHtml).join("");
  if (crumbs.length > 3 && pills.scrollWidth > pills.clientWidth + 1) {
    pills.innerHTML = lead + [crumbs[0], "…", ...crumbs.slice(-2)].map(crumbHtml).join("");
  }
}

function buildFooter(surface, picker) {
  const footer = surface.querySelector(".penna-palette-footer");
  footer.insertAdjacentHTML(
    "beforeend",
    picker
      .hints()
      .map(
        (hint) =>
          `<span class="penna-palette-hint zz-hint"><span class="penna-palette-keys"><span class="penna-palette-glyphs" data-kbd="${hint.keys}"></span></span><span class="zz-words">${hint.words.map((word) => `<span data-word="${word}">${word}</span>`).join("")}</span></span>`,
      )
      .join(""),
  );
  globalThis.pennaKbd?.fill(footer);
  return [...footer.querySelectorAll(".zz-hint")];
}

function paint(surface, picker, lead = "") {
  surface.slots ??= buildFooter(surface, picker);
  fitCrumbs(surface.querySelector(".penna-palette-pills"), lead, picker.crumbs);
  const list = surface.querySelector(".penna-palette-rows");
  const selected = picker.row;
  list.innerHTML = picker.entries.length
    ? picker.entries
        .map((entry) => {
          if (entry.header) return `<div class="penna-palette-section"><span>${esc(entry.header)}</span></div>`;
          if (entry.rule) return `<div class="penna-separator"></div>`;
          return rowHtml(entry, entry === selected);
        })
        .join("")
    : `<div class="penna-palette-empty">No matches</div>`;
  const shown = list.querySelector(`[aria-selected="true"]`);
  if (shown) {
    const box = list.getBoundingClientRect();
    const at = shown.getBoundingClientRect();
    if (at.top < box.top) list.scrollTop -= box.top - at.top;
    else if (at.bottom > box.bottom) list.scrollTop += at.bottom - box.bottom;
  }
  picker.hints().forEach((hint, index) => {
    const slot = surface.slots[index];
    if (hint.on) slot.removeAttribute("data-state");
    else slot.setAttribute("data-state", "disabled");
    slot.querySelectorAll("[data-word]").forEach((word) => word.toggleAttribute("data-on", word.dataset.word === hint.word));
  });
}

function press(event, picker, empty, rootBackspace) {
  const key = event.key;
  const chord = event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
  if (key === "ArrowDown" || (chord && key === "n")) return { outcome: picker.move(1) };
  if (key === "ArrowUp" || (chord && key === "p")) return { outcome: picker.move(-1) };
  if (key === "Enter" && event.altKey) return { outcome: picker.copy() };
  if (key === "Enter") return { outcome: picker.take() };
  if (key === "Tab") return { outcome: event.shiftKey ? picker.back() : picker.tab() };
  if (key === "Escape") return { escape: true };
  if (key === "Backspace" && empty && (picker.path.length > 1 || rootBackspace)) return { outcome: picker.back() };
  return null;
}

function clip(text) {
  navigator.clipboard?.writeText(text).catch(() => {});
}

function wireRows(surface, picker, input, render) {
  surface.addEventListener("mousedown", (event) => {
    if (event.target === input) return;
    event.preventDefault();
    input.focus();
    const row = event.target.closest(".penna-palette-row");
    if (!row) return;
    picker.select(row.dataset.key);
    render();
  });
}

function mountField(stage, picker) {
  const surface = stage.querySelector(".penna-palette-surface");
  const input = surface.querySelector("input");
  const readout = stage.querySelector(".zz-readout");
  const render = () => paint(surface, picker);
  input.addEventListener("input", () => {
    picker.type(input.value);
    input.value = picker.query;
    render();
  });
  input.addEventListener("keydown", (event) => {
    const hit = press(event, picker, input.value === "", true);
    if (!hit) return;
    event.preventDefault();
    event.stopPropagation();
    if (hit.escape) {
      picker.reset();
      input.value = "";
      input.blur();
      render();
      return;
    }
    const out = hit.outcome;
    if (out?.kind === "take") {
      readout.textContent = out.text;
      picker.reset();
    }
    if (out?.kind === "copy") {
      readout.textContent = `copied → ${out.text}`;
      clip(out.text);
    }
    input.value = picker.query;
    render();
  });
  wireRows(surface, picker, input, render);
  render();
  return render;
}

function mountMention(stage, picker) {
  const menu = stage.querySelector(".penna-palette-surface");
  const input = stage.querySelector(".zz-compose > input");
  const partsEl = stage.querySelector(".zz-parts");
  const readout = stage.querySelector(".zz-readout");
  const parts = [];
  let open = true;
  let at = 0;
  let length = 0;
  input.value = "@";
  const query = () => input.value.slice(at + 1, at + 1 + length);
  const sync = () => {
    input.value = swapQuery(input.value, at, length, picker.query);
    length = picker.query.length;
    input.setSelectionRange(at + 1 + length, at + 1 + length);
  };
  const render = () => {
    menu.hidden = !open;
    if (open) paint(menu, picker, `<span class="zz-at">@</span>`);
  };
  const drawParts = () => {
    partsEl.innerHTML = parts
      .map((part) =>
        part.chip
          ? `<span class="penna-badge zz-mention" data-penna="badge"><i data-icon="${part.icon}"></i><span>${esc(part.chip)}</span></span>`
          : `<span class="zz-text">${esc(part.text)}</span>`,
      )
      .join("");
  };
  const close = () => {
    open = false;
    picker.reset();
  };
  input.addEventListener("input", (event) => {
    const caret = input.selectionStart;
    const value = input.value;
    if (open && (value[at] !== "@" || caret <= at)) close();
    if (!open) {
      const found = event.data?.includes("@") ? mentionStart(value, caret) : -1;
      if (found >= 0) {
        open = true;
        at = found;
        length = 0;
        picker.reset();
      }
    }
    if (open) {
      length = caret - at - 1;
      picker.type(query());
      sync();
    }
    render();
  });
  input.addEventListener("keydown", (event) => {
    if (!open) {
      if (event.key === "Escape") input.blur();
      if (event.key === "Backspace" && input.selectionStart === 0 && input.selectionEnd === 0 && parts.length) {
        event.preventDefault();
        const last = parts.pop();
        if (!last.chip) input.value = last.text + input.value;
        drawParts();
      }
      return;
    }
    const hit = press(event, picker, query() === "", false);
    if (!hit) return;
    event.preventDefault();
    event.stopPropagation();
    if (hit.escape) {
      close();
      input.blur();
      render();
      return;
    }
    const out = hit.outcome;
    if (out?.kind === "take") {
      const { before, after } = cutQuery(input.value, at, length);
      if (before) parts.push({ text: before });
      parts.push({ chip: out.text, icon: out.row.icon });
      input.value = after;
      input.setSelectionRange(0, 0);
      close();
      drawParts();
    } else {
      if (out?.kind === "copy") {
        readout.textContent = `copied → ${out.text}`;
        clip(out.text);
      }
      sync();
    }
    render();
  });
  stage.addEventListener("mousedown", (event) => {
    if (event.target === input) return;
    if (!event.target.closest(".zz-chat")) return;
    event.preventDefault();
    input.focus();
    const row = event.target.closest(".penna-palette-row");
    if (!row || !open) return;
    picker.select(row.dataset.key);
    render();
  });
  render();
  return render;
}

function boot() {
  const pickers = [];
  const renders = [];
  const mount = (name, mounter) => {
    const picker = new Picker(sources[name]());
    pickers.push(picker);
    renders.push(mounter(document.querySelector(`[data-picker="${name}"]`), picker));
  };
  mount("palette", mountField);
  mount("mention", mountMention);
  mount("files", mountField);

  const modes = ["nothing", "take"];
  const control = document.querySelector("[data-tab-mode]");
  const fromHash = () => (/(^|[#&])tab=take(&|$)/.test(location.hash) ? 1 : 0);
  const apply = (index) => {
    pickers.forEach((picker) => (picker.tabMode = modes[index]));
    renders.forEach((render) => render());
  };
  control.addEventListener("change", (event) => {
    history.replaceState(null, "", `#tab=${modes[event.detail.index]}`);
    apply(event.detail.index);
  });
  addEventListener("hashchange", () => {
    globalThis.pennaSegmented?.set(control, fromHash());
    apply(fromHash());
  });
  globalThis.pennaSegmented?.set(control, fromHash());
  apply(fromHash());
  const again = () => renders.forEach((render) => render());
  addEventListener("resize", again);
  document.fonts?.ready.then(again);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && again(), true);
}

if (typeof document !== "undefined") boot();
