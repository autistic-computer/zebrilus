import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const revision = '36e584d59152181927e0d8bf70fddcd2a5f33d6c';
const crate = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const source = process.env.PELE_SOURCE;
if (!source) throw new Error('Set PELE_SOURCE to the pinned Pele checkout, built with npm run build.');
const actual = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (actual !== revision) throw new Error(`Expected Pele ${revision}, found ${actual}.`);
const pele = await import(pathToFileURL(path.join(source, 'dist/pele.js')).href);

const seeds = new Set();
const specs = path.join(source, 'tests/compat/state');
for (const file of fs.readdirSync(specs).filter((name) => /\.spec\.[jt]s$/.test(name)).sort()) {
  const text = fs.readFileSync(path.join(specs, file), 'utf8');
  for (const m of text.matchAll(/`((?:[^`\\]|\\.)*)`|'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"/g)) {
    const raw = m[1] ?? m[2] ?? m[3];
    if (!/stateDiagram/.test(raw)) continue;
    try {
      seeds.add(new Function('return `' + raw.replace(/`/g, '\\`').replace(/\$\{/g, '\\${') + '`')());
    } catch {
      continue;
    }
  }
}
for (const sample of JSON.parse(fs.readFileSync(path.join(source, 'tests/corpus/state-docs.json'), 'utf8'))) seeds.add(sample);

const extra = [
  'stateDiagram-v2\n    [*] --> Idle\n    Idle --> Running : start\n    state Running {\n        [*] --> Working\n        Working --> Waiting : tool call\n        Waiting --> Working : result\n    }\n    Running --> Idle : stop\n    Running --> [*]',
  'stateDiagram-v2\nstate a0 {\nstate a1 {\n}\n}\nstate a1 {\nstate a2 {\n}\n}\nstate a2 {\nstate a0 {\n}\n}\nx --> a1',
  'stateDiagram-v2\nstate root {\n a --> b\n --\n c --> d\n}\nroot --> root\n',
  'stateDiagram-v2\n A --> B\n note left of A : left note\n note right of B\n  multi\n  line **bold** note\n end note\n note right of A: second',
  'stateDiagram-v2\n A --> B\n click A "https://Example.com" "Tip &amp; more"\n click B href "javascript:alert(1)"',
  'stateDiagram-v2\n A --> B\n click A href "https://example.com/a b?q=1 2#f g"\n click B "//evil.com" "t"',
  'stateDiagram-v2\n A --> B\n B --> C\n click A "mailto:me@example.com" "mail"\n click B "/relative/path" "rel"\n click C "file:///etc/passwd" "x"',
  'stateDiagram-v2\n A --> B\n click A "&#106;avascript:alert(1)" "x"\n click B "data:text/html,x" "y"',
  'stateDiagram-v2\n state "Long description" as y:extra\n y --> z',
  'stateDiagram-v2\n state C {\n --\n a --> b\n --\n }',
  'stateDiagram-v2\n state C {\n a --> a : self one\n a --> a : self two\n }\n C --> C : outer loop\n C --> C',
  'stateDiagram-v2\n state C {\n a --> b\n }\n C --> a\n b --> C\n C --> b',
  'stateDiagram-v2\n direction LR\n state C {\n direction TB\n a --> b\n }\n x --> C\n C --> y\n state f <<fork>>\n state j <<join>>\n y --> f\n f --> p\n f --> q\n p --> j\n q --> j\n j --> [*]',
  'stateDiagram-v2\n direction RL\n a --> b\n state c <<choice>>\n b --> c\n c --> d : yes\n c --> e : no',
  'stateDiagram-v2\n direction BT\n a --> b\n state B {\n direction LR\n x --> y\n y --> z\n }\n b --> B',
  'stateDiagram-v2\n hide empty description\n scale 350 width\n accTitle: The title\n accDescr {\n line one\n   line two\n }\n a --> b',
  'stateDiagram-v2\n a --> b : Price #35; #quot;quoted#quot;\n b : value #lt;tag#gt;',
  'stateDiagram-v2\n classDef hot fill:#f00;\n classDef cold fill:#00f,color:#fff;stroke:#0ff\n a --> b\n class a hot\n class b cold\n style a stroke:#0f0,stroke-width:3px',
  'stateDiagram-v2\n a:::hot --> b:::cold\n classDef hot fill:#000,font-weight:bold\n classDef cold font-style:italic,font-size:20px',
  'stateDiagram-v2\n Привет --> 世界 : 你好\n 世界 --> café',
  'stateDiagram-v2\n a : line one\n a : line two\n a : \n a --> b',
  'stateDiagram-v2\n state Big {\n state Inner {\n note right of x : inside note\n x --> y\n }\n }\n Big --> out',
  'stateDiagram-v2\n state "Composite with a fairly long title" as Q {\n s1 --> s2\n }\n start --> Q\n Q --> s1',
  'stateDiagram-v2\n state P {\n a\n --\n b\n --\n c\n }\n state Q {\n direction LR\n d\n --\n e\n }',
  'stateDiagram-v2\n state fork1 <<fork>>\n [*] --> fork1\n fork1 --> a\n fork1 --> b\n fork1 --> c\n fork1 --> d\n fork1 --> e\n state join1 <<join>>\n a --> join1\n b --> join1\n c --> join1\n d --> join1\n e --> join1',
  'stateDiagram-v2\n a --> b\n a --> b\n a --> b : third\n b --> a',
  'stateDiagram-v2\n a --> \n',
  'stateDiagram-v2\n state a b {\n }',
  'stateDiagram-v2\n note right of a\n never ends',
  'stateDiagram-v2\n }',
  'stateDiagram-v2\n state S {\n a --> b\n',
  'stateDiagram-v2\n state Comp {\n x --> y\n }\n state "a" as Comp:extra\n',
];
for (const sample of extra) seeds.add(sample);

const base = { fontSize: 16, fontFamily: 'Arial', padding: 8, responsive: false };
const variants = [
  [/direction (LR|RL)/, { maxWidth: 320 }],
  [/state .*\{/, { config: { flowchart: { curve: 'step' }, state: { nodeSpacing: 24, rankSpacing: 60 } } }],
  [/-->.*:/, { fontSize: 13, padding: 3, config: { flowchart: { curve: 'linear' }, state: { wrappingWidth: 60 } } }],
];
const colors = {
  '--_bg': '#ffffff',
  '--_fg': '#1f1f1f',
  '--_m': '#6e6e6e',
  '--_l': '#1f1f1f',
  '--_s': '#f3f3f3',
  '--_a': '#e6e6e6',
  '--_b': '#8a8a8a',
  '--_c': '#5b7bd5',
  '--_w': 'bold',
  '--_tw': 'bold',
  '--_fm': 'monospace',
};
const body = (svg) =>
  svg
    .replace(/^<svg[^>]*>(<title[^<]*<\/title>)?(<desc[^<]*<\/desc>)?/, '')
    .replace(/<\/svg>$/, '')
    .replace(/ style="rx:var\(--_r\)"/g, ' rx="4"')
    .replace(/var\(--pele-fg-on-light,#1f1f1f\)/g, '#1f1f1f')
    .replace(/var\(--pele-fg-on-dark,#fff\)/g, '#fff')
    .replace(/var\((--_\w+)\)/g, (m, name) => colors[name] ?? m)
    .replace(/(<text[^>]*?) font-size="[^"]*"/g, '$1');

const cases = [];
const record = (text, options) => {
  let expected;
  try {
    const result = pele.render(text, options);
    expected = { ok: true, width: result.width, height: result.height, body: body(result.svg), links: result.links };
  } catch (error) {
    expected = { ok: false, code: error.code, line: error.line, column: error.column };
  }
  cases.push({ id: `state-${cases.length}`, source: text, options, expected });
};
for (const text of seeds) {
  record(text, base);
  for (const [pattern, extraOptions] of variants) if (pattern.test(text)) record(text, { ...base, ...extraOptions });
}

const out = path.join(crate, 'tests/fixtures/state.json');
fs.writeFileSync(out, JSON.stringify({ upstreamRevision: revision, cases }) + '\n');
console.log(`Wrote ${cases.length} cases to ${path.relative(crate, out)}.`);
