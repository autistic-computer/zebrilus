import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const revision = '36e584d59152181927e0d8bf70fddcd2a5f33d6c';
const source = process.env.PELE_SOURCE;
if (!source) throw new Error('Set PELE_SOURCE to the pinned Pele checkout, built with npm run build.');
const actual = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (actual !== revision) throw new Error(`Expected Pele ${revision}, found ${actual}.`);
const pele = await import(pathToFileURL(path.join(source, 'dist/pele.js')).href);
const options = { fontSize: 16, fontFamily: 'Arial', padding: 8, responsive: false, now: 1793232000000 };

const docs = JSON.parse(fs.readFileSync(path.join(source, 'tests/corpus/class-docs.json'), 'utf8'));
const literals = new Set(docs);
const specs = path.join(source, 'tests/compat/class');
for (const file of fs.readdirSync(specs).filter((name) => /\.spec\.[jt]s$/.test(name)).sort()) {
  const text = fs.readFileSync(path.join(specs, file), 'utf8');
  for (const m of text.matchAll(/`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g)) {
    const raw = m[1] ?? m[2] ?? m[3];
    try {
      literals.add(new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')());
    } catch {
      continue;
    }
  }
}
const pieces = [...literals].filter((s) => !s.includes('classDiagram'));
const corpus = [
  ...[...literals].filter((s) => s.includes('classDiagram')),
  ...pieces.filter((s) => !/^(?:should|when|given|does|gives|emits|moves|defaults) /.test(s)).map((s) => 'classDiagram\n' + s),
];

function tour() {
  const out = [];
  const names = ['A', 'A.B', 'A-B', 'A B', '`a b`', 'A`b`', 'A~T~', '`a`~T~', 'A.`b`~T~', 'é', '1', 'A.', '.A', 'A~T~~U~', '`a`B', 'o', 'link'];
  for (const a of names) {
    out.push(a, `${a} : label`, `class ${a}`, `namespace ${a} {\nclass X\n}`, `<<i>> ${a}`, `note for ${a} "n"`, `click ${a} href "u"`);
    for (const b of ['A', 'B~T~', '`b`']) out.push(`${a} --> ${b}`, `${a} "1" --> "2" ${b} : t`);
  }
  const ends = ['', '<|', '|>', '<', '>', '*', 'o', '()'];
  for (const left of ends) {
    for (const line of ['--', '..', '-', '']) {
      for (const right of ends) out.push(`A ${left}${line}${right} B`, `A "1" ${left}${line}${right} "2" B : t`, `A ${left}${line}${right}`);
    }
  }
  const bodies = ['', ' {\n}', '{}', '{ x }', ' {\n+a\n-b()\n}', ' {\n"q"\n}', ' {\n[*]\n}', ' { {', ' {\n+a', ' }'];
  for (const label of ['', '["L"]', '[L]', '["L"']) {
    for (const mid of ['', ':::c', ':::', ' <<i>>', ' <<i', ' : x', ' B']) {
      for (const body of bodies) out.push(`class A${label}${mid}${body}`, `namespace N {\nclass A${label}${mid}${body}\n}`);
    }
  }
  const items = ['class A', 'class B {\n+x\n}', 'note "n"', 'note for A "n"', 'namespace M {\nclass C\n}', 'A --> B', ''];
  for (const label of ['', '["L"]']) {
    for (const open of ['{', '{\n', ' {\n\n']) {
      for (const first of items) {
        for (const sep of ['\n', ' ', '\n\n']) {
          for (const second of items) out.push(`namespace N${label} ${open}${first}${sep}${second}}`, `namespace N${label} ${open}${first}${sep}${second}\n}\nclass Z`);
        }
      }
    }
  }
  out.push('namespace A {\nnamespace B {\nnamespace C {\nclass X\n}\n}\nclass Y\n}', 'namespace A { namespace B { class X } }', 'namespace A {\nclass X\n}}', 'namespace namespace A {\nclass X\n}');
  for (const head of ['callback A', 'link A', 'click A href', 'click A call f()', 'click A call f(1, "a")', 'click A call f', 'click A']) {
    for (const tail of ['', ' "a"', ' "a" "b"', ' "a" _blank', ' "a" "b" _self', ' "a" "b" "c"', ' _top', ' "a" _parent "b"']) out.push(head + tail);
  }
  for (const head of ['style A', 'style', 'classDef a', 'classDef a,b', 'classDef a,', 'classDef', 'cssClass "A"', 'cssClass "A,B"', 'cssClass A']) {
    for (const tail of ['', ' fill:#f9f', ' fill:#f9f,stroke:#333,stroke-width:4px', ' x,y', ' c', ' x:1;', ' a % # : style', ' "s"']) out.push(head + tail);
  }
  for (const text of ['"n"', '', 'n', '"a" "b"', '""']) out.push(`note ${text}`, `note for A ${text}`, `note for ${text}`, `notefor A ${text}`);
  for (const line of [
    'direction TB', 'direction BT', 'direction RL', 'direction LR', 'direction TD', 'x direction LR y', 'direction\nTB', 'direction LR direction TB',
    'accTitle: t', 'accTitle:', 'accTitle : t', 'accDescr: d', 'accDescr { a\nb }', 'accDescr {}', 'accDescr { a', 'accTitle t',
    '<<i>> A', '<<i>>', '<<i> A', '<< i >> A~T~', '%% c', 'A %% c', '[*] --> A', 'A --> [*]', 'call f()', 'A::B', 'A:::c', 'A;',
  ]) {
    out.push(line, `${line}\nclass A`, `class A\n${line}`);
  }
  return out.map((s) => 'classDiagram\n' + s);
}

const FRAGMENTS = [
  ' ', '\n', '\r\n', '\t', ';', ':', '::', ':::', ': x', ',', '.', '..', '-', '--', '<|', '|>', '<', '>', '<<', '>>',
  '*', 'o', ' o ', '()', '(', ')', '{', '}', '{\n', '\n}', '[', ']', '[*]', '"', '""', '"x"', '`', '`a b`', '~', '~T~',
  '#', '%', '%%', '%% c\n', '+', '=', '!', '$', '&', '?', '/', '\\', '_', 'A', 'B', 'Class01', '1', 'é', 'ö', '中',
  'class ', 'class', 'classDiagram', 'classDiagram-v2', 'classDef ', 'cssClass ', 'style ', 'namespace ',
  'namespace N {\n', 'note ', 'note for ', 'note for A "x"', 'link ', 'click ', 'callback ', 'href ', 'call ',
  'call f()', 'call f(1, "a")', '_self', '_blank', '_parent', '_top', 'direction TB', 'direction LR', 'direction',
  'accTitle: ', 'accDescr: ', 'accDescr {', '<|--', '--|>', '*--', '--*', 'o--', '--o', '-->', '<--', '..>', '<..',
  '..|>', '<|..', '()--', '--()', '"1" ', ' "many"', '<<interface>>', 'fill:#f9f', 'stroke-width:2px', '+foo()',
  '-bar: int', 'x(y) z$', ' \n', '}\n', 'A.B', 'a-b', '#35;', '&amp;', 'List~int~',
];

function mutations(seeds, count) {
  let state = 7;
  const next = () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 4294967296;
  };
  const pick = (list) => list[Math.floor(next() * list.length)];
  const out = [];
  for (let i = 0; i < count; i++) {
    let s = pick(seeds);
    const edits = 1 + Math.floor(next() * 3);
    for (let k = 0; k < edits; k++) {
      const at = Math.floor(next() * (s.length + 1));
      const r = next();
      if (r < 0.4) s = s.slice(0, at) + pick(FRAGMENTS) + s.slice(at);
      else if (r < 0.65) s = s.slice(0, at) + s.slice(at + 1 + Math.floor(next() * 3));
      else if (r < 0.85) s = s.slice(0, at) + s.slice(at, at + Math.floor(next() * 6)) + s.slice(at);
      else s = s.slice(0, at) + pick(FRAGMENTS) + s.slice(at + 1);
    }
    if (s.length <= 2000 && s.includes('classDiagram')) out.push(s);
  }
  return out;
}

const repeat = (count, line) => Array.from({ length: count }, (_, i) => line(i)).join('');
const diagram = (body) => 'classDiagram\n' + body;
const urls = [
  'javascript:alert(1)', 'JaVaScRiPt:alert(1)', 'data:text/html,x', 'vbscript:x', '//evil.example', '\\\\evil.example',
  '%6aavascript%3aalert(1)', 'javascript&colon;alert(1)', 'java&#115;cript:alert(1)', '&#x6a;avascript:alert(1)',
  'https://Example.COM:443/a b?q=1 2#f g', 'HTTPS://Example.com', 'https:example.com/path', 'http://example.com:80',
  'https://example.com:8080/x/../y/./z', 'https://user:pw@host.example/p', 'https://ex.example/é?ü#ö',
  'https://[::1]:8080/x', 'https://ex.example/a\\b', 'https://ex.example/<tag>', ' https://padded.example ',
  'mailto:a@b.example', 'tel:+123', 'ftp://x.example/z', 'file:///etc/passwd', 'about:blank', './rel/path', '/abs',
  '#anchor', 'note.md', 'custom://open?x=1',
];
const extras = [
  'classDiagram\n    class AgentTimeline {\n        +rows\n        +active_turn\n        +render()\n    }\n    class AgentTimelineStore {\n        +markdown\n        +expanded\n        +set_streaming()\n    }\n    class ListState {\n        +scroll_to_end()\n    }\n    AgentTimeline --> AgentTimelineStore : reads\n    AgentTimeline --> ListState : scrolls',
  ...urls.map((url) => diagram(`class A\nlink A "${url}" "tip"`)),
  ...urls.map((url) => diagram(`class A\nclick A href "${url}" _top`)),
  diagram('class A["#35; &amp; #lt; #quot;"]\nA : +x #35;1\nA --> B : a #9829; b'),
  diagram('class A\nstyle A fill:#f9f;stroke:#333;'),
  diagram('class A:::hot\nclassDef hot fill:#f00;color:#fff;'),
  diagram('class A:::k\nclassDef k fill:#000,color:#fff,font-size:20px,font-weight:bold,font-style:italic\nA : +a$\nA : +b()*\nA : c'),
  diagram('class A\nstyle A fill:#fff,stroke:#f00,font-family:Courier\nA : +$x\nA : +y() int$'),
  diagram('class A~T~\nclass B~List~int~~\nA~T~ <|-- B\nnote for A "generic note"'),
  diagram(repeat(8, (i) => `Hub "1" --> "${i}..*" C${i} : rel${i}\n`)),
  diagram(repeat(6, (i) => `C${i} "many" --> "one" Hub\n`)),
  diagram('A "1" --> "2" A : self\nA "x" --> "y" A\nA --> A : again'),
  diagram('direction LR\nnamespace Outer {\n  class A\n  class B\n}\nnamespace Other {\n  class C\n}\nA --> C : crosses\nB --> C\nC --> A'),
  diagram('direction RL\nnamespace N1 {\nclass A\n}\nnamespace N2 {\nclass B\n}\nA "1" --> "n" B : x'),
  diagram('direction BT\nA <|-- B\nA <|-- C\nB *-- D\nC o-- D'),
  '---\ntitle: Titled\nconfig:\n  class:\n    hideEmptyMembersBox: true\n---\nclassDiagram\naccTitle: Accessible\naccDescr: Described\nclass A\nclass B {\n+x\n}\nA --> B',
  '---\nconfig:\n  class:\n    hierarchicalNamespaces: false\n    nodeSpacing: 80\n    rankSpacing: 20\n---\nclassDiagram\nnamespace A.B.C {\nclass X\n}\nnamespace A {\nclass Y\nnote "in A"\n}\nX --> Y',
  diagram('namespace N {\nnote "n1"\nnote for X "n2"\nclass X\n}\nfoo ()-- X\nX --() bar'),
  diagram('class A\nclass B\nclass C\nclass D\nclass E\nclass F\nA --> B\nC --> D\nE --> F\nnote "free"'),
  diagram(repeat(12, (i) => `class C${i} {\n+field${i} int\n+method${i}() void\n}\n`)),
  diagram('accDescr {\n  line one\n    line two\n}\nclass A'),
  diagram('class A {\n+List~Map~K,V~~ items\n+get(K key) V\n+put~T~(T t)$\n-run)(\n}'),
  diagram('`Back Tick` --> `Other One` : "q"'),
  diagram('A "<b>1</b>" --> "&lt;2&gt;" B : <i>html</i> label'),
  diagram('note "multi\nline\nnote with **bold** and _it_"'),
  diagram('class Unicode_é中 {\n+名前 string\n}'),
  diagram(repeat(30, (i) => `A${i % 5} --> B${i % 7}\n`)),
  diagram('click A call cb()\nclass A\nclick A call cb() "tip"\ncallback A "fn" "tip2"\ncssClass "A" hot'),
];

const colors = { '--_bg': '#ffffff', '--_fg': '#1f1f1f', '--_m': '#6e6e6e', '--_l': '#1f1f1f', '--_s': '#f3f3f3', '--_a': '#e6e6e6', '--_b': '#8a8a8a', '--_w': 'bold', '--_tw': 'bold', '--_hw': 'bold', '--_fm': 'monospace' };
const mix = (share) => '#' + [0xe6, 0xe6, 0xe6].map((x) => Math.round(x * share + 0xff * (1 - share)).toString(16).padStart(2, '0')).join('');
const body = (svg) => svg
  .replace(/^<svg[^>]*>/, '')
  .replace(/<\/svg>$/, '')
  .replace(/^(<title[^<]*<\/title>)?(<desc[^<]*<\/desc>)?/, '')
  .replace(/ style="rx:var\(--_r\)"/g, ' rx="4"')
  .replace(/ style="rx:var\(--_r\);([^"]*)"/g, ' style="$1" rx="4"')
  .replace(/var\(--pele-fg-on-light,#1f1f1f\)/g, '#1f1f1f')
  .replace(/var\(--pele-fg-on-dark,#fff\)/g, '#fff')
  .replace(/color-mix\(in srgb,var\(--_a\) ([0-9.]+)%,var\(--_bg\)\)/g, (_, share) => mix(Number(share) / 100))
  .replace(/var\((--_\w+)\)/g, (m, name) => colors[name] ?? m)
  .replace(/(<text[^>]*?) font-size="1[46]"/g, '$1');
const fnv = (text) => {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) hash = ((hash ^ BigInt(byte)) * 0x100000001b3n) & 0xffffffffffffffffn;
  return hash.toString(16).padStart(16, '0');
};

const cases = [];
const seen = new Set();
const record = (group, text, maxWidth, full) => {
  const key = `${maxWidth ?? ''}\u0000${text}`;
  if (seen.has(key)) return;
  seen.add(key);
  const item = { group, source: text };
  if (maxWidth !== undefined) item.max_width = maxWidth;
  try {
    const result = pele.render(text, maxWidth === undefined ? options : { ...options, maxWidth });
    const markup = body(result.svg);
    if (markup.includes('var(') || markup.includes('color-mix(')) throw new Error(`Unmapped CSS variable in ${JSON.stringify(text)}`);
    item.width = result.width;
    item.height = result.height;
    if (result.links.length > 0) item.links = result.links.map(({ id, href }) => [id, href]);
    item.hash = fnv(markup);
    if (full) item.body = markup;
  } catch (error) {
    if (error.code === undefined) throw error;
    item.error = [error.code, error.line, error.column];
  }
  cases.push(item);
};
for (const text of docs) record('docs', text, undefined, true);
for (const text of docs) record('docs', text, 260, false);
for (const text of extras) record('extras', text);
for (const text of extras) record('extras', text, 300, false);
for (const text of corpus) record('specs', text);
for (const text of tour()) record('tour', text);
for (const text of mutations([...corpus, ...tour()], 1200)) record('mutations', text);

const destination = fileURLToPath(new URL('../tests/fixtures/class-reference.json', import.meta.url));
fs.writeFileSync(destination, JSON.stringify({ upstreamRevision: revision, options, cases }) + '\n');
console.log(`Wrote ${cases.length} class cases, ${cases.filter((c) => c.error).length} of them errors, to ${destination}`);
