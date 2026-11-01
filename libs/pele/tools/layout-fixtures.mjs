import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const revision = '36e584d59152181927e0d8bf70fddcd2a5f33d6c';
const source = process.env.PELE_SOURCE;
if (!source) throw new Error('Set PELE_SOURCE to the pinned Pele checkout with npm dependencies installed.');
const actual = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (actual !== revision) throw new Error(`Expected Pele ${revision}, found ${actual}.`);
const { build } = await import(pathToFileURL(path.join(source, 'node_modules/esbuild/lib/main.js')).href);
const bundled = await build({
  absWorkingDir: source,
  entryPoints: ['src/layout/layered.ts'],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
});
const reference = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);
const seed = 123456789;
let state = seed;
const next = () => state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
const cases = [];
for (let k = 0; k < 120; k++) {
  const n = 2 + next() % 10;
  const nodes = Array.from({ length: n }, () => {
    const node = reference.lnode(30 + next() % 100, 20 + next() % 45);
    if (k % 3 !== 0) node.span = node.w - 10;
    return node;
  });
  const edges = [];
  for (let a = 0; a < n; a++) {
    for (let b = a + 1; b < n; b++) {
      if (next() % 5 === 0) edges.push(reference.ledge(a, b, 1, k % 4 === 0 ? 20 + next() % 40 : 0, k % 4 === 0 ? 18 : 0));
    }
  }
  if (edges.length === 0) edges.push(reference.ledge(0, n - 1));
  if (k % 5 === 0 && n > 2) edges.push(reference.ledge(n - 1, 0));
  const opt = {
    nodeSep: 40,
    edgeSep: 20,
    rankSep: 50,
    portSep: 20,
    tracks: k % 4 === 1,
    straight: k % 7 === 0,
    headRoom: 10,
  };
  const initial = { nodes: structuredClone(nodes), edges: structuredClone(edges), opt };
  const result = reference.layered(nodes, edges, opt);
  cases.push({ ...initial, expected: { result, nodes, edges } });
}
const destination = fileURLToPath(new URL('../tests/fixtures/layout-reference.json', import.meta.url));
fs.writeFileSync(destination, JSON.stringify({ upstreamRevision: revision, seed, cases }) + '\n');
console.log(`Wrote ${cases.length} layout reference cases to ${destination}`);
