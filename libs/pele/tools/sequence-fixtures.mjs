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
const spec = fs.readFileSync(path.join(source, 'tests/compat/sequence/sequenceDiagram.spec.js'), 'utf8');
for (const m of spec.matchAll(/`((?:[^`\\]|\\.)*)`/g)) {
  const raw = m[1];
  if (/sequenceDiagram/.test(raw) && /^\s*(create|destroy)\s/m.test(raw)) seeds.add(raw.replace(/^\n/, ''));
}
for (const sample of JSON.parse(fs.readFileSync(path.join(source, 'tests/corpus/sequence-docs.json'), 'utf8'))) {
  if (/^\s*(create|destroy)\s/m.test(sample)) seeds.add(sample);
}

const extra = [
  'sequenceDiagram\n participant A\n participant B\n create participant C\n B->>C: spawn\n C-->>A: ready\n A->>B: thanks',
  'sequenceDiagram\n participant A\n create participant B as The worker\n A->>+B: start\n B-->>-A: done\n destroy B\n A-xB: stop',
  'sequenceDiagram\n participant Left\n participant Right\n create actor C\n Right->>C: hello\n C->>Left: across',
  'sequenceDiagram\n autonumber\n A->>B: one\n create participant C\n B->>C: two\n destroy C\n C->>B: three\n B->>A: four',
  'sequenceDiagram\n A->>B: hi\n create participant C\n note over C: waiting\n B->>C: wake',
  'sequenceDiagram\n A->>B: hi\n destroy B\n A->>B: again\n A->>B: still drawn',
  'sequenceDiagram\n A->>B: hi\n create participant C\n B->>C: hello\n destroy C\n B->>C: bye\n destroy A\n B->>A: bye too',
  'sequenceDiagram\n box Team\n participant A\n participant B\n end\n create participant C\n A->>C: join\n destroy B\n B->>A: leave',
  'sequenceDiagram\n A->>B: hi\n loop polling\n create participant C\n B->>C: ask\n destroy C\n C-->>B: answer\n end',
  'sequenceDiagram\n A->>B: hi\n destroy B',
  'sequenceDiagram\n A->>B: hi\n create participant B\n A->>B: again',
  'sequenceDiagram\n create participant C\n A->>B: hi',
  'sequenceDiagram\n A->>B: hi\n destroy B\n A->>C: hi',
  'sequenceDiagram\n box Grp\n create participant C\n end\n A->>C: hi',
];
for (const sample of extra) seeds.add(sample);

const base = { fontSize: 16, fontFamily: 'Arial', padding: 8, responsive: false };
const variants = [[/create actor/, { config: { sequence: { mirrorActors: false } } }]];

const geometry = (svg) => ({
  lifelines: [...svg.matchAll(/<path class="pele-lifeline" data-id="([^"]*)" d="([^"]*)"/g)].map((m) => `${m[1]} ${m[2]}`).sort(),
  destroys: [...svg.matchAll(/<path class="pele-destroy" data-id="([^"]*)" d="([^"]*)"/g)].map((m) => `${m[1]} ${m[2]}`).sort(),
  actors: [...svg.matchAll(/<g class="pele-node pele-actor[^"]*" data-id="([^"]*)"><(rect|circle) ([^>]*?)\/?>/g)]
    .map((m) => `${m[1]} ${m[2]} ${m[3].replace(/ ?(fill|stroke|class|rx|style)="[^"]*"/g, '').trim()}`)
    .sort(),
  messages: [...svg.matchAll(/<g class="pele-edge pele-message"[^>]*><path d="([^"]*)"/g)].map((m) => m[1]),
});

const cases = [];
const record = (text, options) => {
  let expected;
  try {
    const result = pele.render(text, options);
    expected = { ok: true, width: result.width, height: result.height, ...geometry(result.svg) };
  } catch (error) {
    expected = { ok: false, code: error.code };
  }
  cases.push({ id: `sequence-${cases.length}`, source: text, options, expected });
};
for (const text of seeds) {
  record(text, base);
  for (const [pattern, extraOptions] of variants) if (pattern.test(text)) record(text, { ...base, ...extraOptions });
}

const out = path.join(crate, 'tests/fixtures/sequence.json');
fs.writeFileSync(out, JSON.stringify({ upstreamRevision: revision, cases }) + '\n');
console.log(`Wrote ${cases.length} cases to ${path.relative(crate, out)}.`);
