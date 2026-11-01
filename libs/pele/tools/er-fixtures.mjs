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

const base = { fontSize: 16, fontFamily: 'Arial', padding: 8, responsive: false };
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
    .replace(/^<svg[^>]*>/, '')
    .replace(/<\/svg>$/, '')
    .replace(/^(<title[^<]*<\/title>)?(<desc[^<]*<\/desc>)?/, '')
    .replace(/ style="rx:var\(--_r\)"/g, ' rx="4"')
    .replace(/ style="rx:var\(--_r\);([^"]*)"/g, ' style="$1" rx="4"')
    .replace(/var\(--pele-fg-on-light,#1f1f1f\)/g, '#1f1f1f')
    .replace(/var\(--pele-fg-on-dark,#fff\)/g, '#fff')
    .replace(/var\((--_\w+)\)/g, (match, name) => colors[name] ?? match)
    .replace(/(<text[^>]*?) font-size="[^"]*"/g, '$1');

const docs = JSON.parse(fs.readFileSync(path.join(source, 'tests/corpus/er-docs.json'), 'utf8'));
const features = [
  'erDiagram\n    USER ||--o{ SESSION : owns\n    SESSION ||--|{ PANE : contains\n    USER {\n        string id PK\n        string email UK\n        datetime created_at\n    }\n    SESSION {\n        string id PK\n        string user_id FK "owner"\n    }',
  'erDiagram\n  EMPLOYEE ||--o{ EMPLOYEE : manages\n  EMPLOYEE }o--|| EMPLOYEE : "reports to"\n  EMPLOYEE {\n    int id PK\n    string name\n  }',
  'erDiagram\n  direction LR\n  NODE ||--o{ NODE : parent\n  NODE |o--o| NODE : sibling\n  NODE }|..|{ NODE : peers\n  NODE ||--|| OTHER : x',
  'erDiagram\n  A u--|| B : md\n  E |o--o| F : zz\n  G }o..o{ H : lots',
  'erDiagram\n  subgraph S1 [Group A]\n    A ||--o{ B : has\n  end\n  style S1 fill:#334,stroke:#f00\n  B ||--|| C : out\n  S1 ||--o{ S1 : self',
  'erDiagram\n  subgraph S1\n    A ||--o{ B : has\n    B ||--|| C : inner\n  end\n  style S1 fill:#ffeecc\n  classDef hot fill:#f00,color:#fff\n  class A hot',
  'erDiagram\n  NOTE:::internal-link\n  "javascript:alert(1)":::internal-link\n  "https://example.com":::internal-link\n  "http:":::internal-link\n  "mailto:a@b.c":::internal-link\n  "a:b":::internal-link\n  "//evil":::internal-link\n  "data:text/html,x":::internal-link\n  " javascript:x":::internal-link\n  "java&#115;cript:x":::internal-link\n  n["My Note"]:::internal-link\n  n ||--o{ NOTE : refs\n  class NOTE,n other',
  'erDiagram\n  THING {\n    List~int~ items\n    Map~string, List~int~~ deep "a tilde comment"\n    string? maybe\n    `weird name` `type x`\n    int a PK, FK, UK "a fairly long comment that should wrap because it is very very long and keeps going on and on beyond the limit"\n  }',
  'erDiagram\n  "Love #9829; it" ||--o{ "B #quot;q#quot;" : "say #35;1"\n  style A fill:#f00;',
  'erDiagram\n  accTitle: My ER\n  accDescr: describes\n  things\n  A ||--o{ B : x',
  'erDiagram\n  subgraph outer\n    direction LR\n    subgraph inner\n      X ||--o{ Y : a\n      Y ||--o{ X : b\n    end\n    Z\n  end\n  outer ||--o{ W : c\n  inner ||--|| Z : d',
  'erDiagram\n  A ||--o{ B : first\n  A ||--o{ B : second\n  A ||--o{ B : third\n  A ||--o{ C : fourth\n  A ||--o{ D : fifth\n  A ||--o{ E : sixth\n  F ||--o{ A : in1\n  G ||--o{ A : in2',
  'erDiagram\n  direction RL\n  A ||--o{ B : x\n  B ||--o{ C : y\n  C ||--o{ D : z',
  'erDiagram\n  direction BT\n  A ||--o{ B : x\n  B ||--o{ C : y',
  'erDiagram\n  1 ||--o{ 2.5 : nums\n  42 ||--|| 1 : back',
  'erDiagram\n  CAR one or more to zero or many DRIVER : x\n  CAR only one optionally to one or zero PLATE : y\n  CAR many(1) to 1+ WHEEL : z\n  CAR 1 to 0+ X : w',
  'erDiagram\n  A["**Bold** _it_ <b>tag</b>"] {\n    string `code` "<i>hi</i>"\n  }',
  'erDiagram\n  A {\n    string a\n  }\n  style A fill:#000,stroke:#0f0,color:#fff,font-size:20px,font-weight:bold',
  'erDiagram\n  classDef default fill:#cfc\n  A ||--o{ B : x\n  A:::foo\n  classDef foo stroke:#f00,stroke-width:3px',
  'erDiagram\n  A ||--o{ B : direction TB here',
  'erDiagram\n  E ||--o{ E : a\n  E ||--o{ E : b\n  E ||--o{ E : c\n  E ||--o{ E : d\n  E ||--o{ E : e',
  'erDiagram\n  direction LR\n  E ||--o{ E : looped\n  E {\n    int id\n  }\n  E ||--o{ F : out',
  'erDiagram\n  direction LR\n  subgraph G\n    E ||--o{ E : self\n    E ||--|| E : again\n  end\n  style G fill:#ffffff\n  G ||--o{ G : grp',
  'erDiagram\n  subgraph S\n    direction LR\n    A ||--o{ A : loopy\n    A ||--o{ B : x\n  end\n  style S fill:#000',
  'erDiagram\n  A ||--o{ B : x\n  subgraph A\n    C\n  end\n  A ||--|| D : y',
  'erDiagram\n  A {\n  }\n  B { int x }',
  'erDiagram\n  ÜBER ||--o{ naïve : ünïcode\n  日本 ||--|| 中文 : 漢字',
  'erDiagram\n  CUSTOMER ||--o{ ORDER : "places<br>many orders over a long period of time and more"',
  "%%{init: {'er': {'nodeSpacing': 120, 'rankSpacing': 20}}}%%\nerDiagram\n  A ||--o{ B : x\n  A ||--o{ C : y\n  A ||--o{ D : z",
];
const errors = [
  'erDiagram\n  A ||--o{',
  'erDiagram\n  A ||--o{ B',
  'erDiagram\n  A {\n    string\n  }',
  'erDiagram\n  A {\n    string name\n',
  'erDiagram\n  subgraph S\n    A\n',
  'erDiagram\n  end',
  'erDiagram\n  A ||--x B : y',
  'erDiagram\n  style A fill:#f00 !\n',
  'erDiagram\n  "👋" ||--o{ @',
  '---\ntitle: T\n---\nerDiagram\n  A ||--o{ B : x\n  C ?',
  'erDiagram\n  A { string a PK, }',
  'erDiagram\n  "#35;#quot; x" ||--o{ B : x\n  C ?',
];

const cases = [];
const add = (origin, text, options) => {
  try {
    const result = pele.render(text, options);
    cases.push({ origin, source: text, options, width: result.width, height: result.height, body: body(result.svg) });
  } catch (error) {
    cases.push({ origin, source: text, options, error: { code: error.code, line: error.line, column: error.column } });
  }
};
docs.forEach((text, index) => add(`er-docs.json:${index}`, text, base));
features.forEach((text, index) => add(`feature:${index}`, text, base));
[...docs, ...features]
  .filter((text) => /direction (LR|RL)/.test(text))
  .forEach((text, index) => add(`narrow:${index}`, text, { ...base, maxWidth: 300 }));
errors.forEach((text, index) => add(`error:${index}`, text, base));

const destination = fileURLToPath(new URL('../tests/fixtures/er.json', import.meta.url));
fs.writeFileSync(destination, JSON.stringify({ upstreamRevision: revision, cases }, null, 1) + '\n');
console.log(`Wrote ${cases.length} ER cases to ${destination}`);
