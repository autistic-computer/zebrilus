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
  stdin: {
    contents: [
      "export * from './src/layout/compound.ts';",
      "export { wrapComponents } from './src/layout/components.ts';",
      "export { routePath, loopPath, marker, markerTrim, overFills, casing } from './src/svg/edges.ts';",
      "export { markCrossings, clusterTitleX, titleCrossed } from './src/svg/cluster.ts';",
    ].join(' '),
    resolveDir: source,
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
});
const reference = await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);
const seed = 987654321;
let state = seed;
const next = () => (state = (Math.imul(state, 1664525) + 1013904223) >>> 0) >>> 8;
const pick = (list) => list[next() % list.length];
const DIRS = ['TB', 'BT', 'LR', 'RL'];
const cases = [];
const paths = [];
const CURVES = [undefined, 'linear', 'step', 'stepBefore', 'stepAfter', 'natural'];
const MARKERS = ['arrow_point', 'arrow_circle', 'arrow_cross', 'none'];
const pathOf = (p) => ({ d: p.d, sx: p.sx, sy: p.sy, sdx: p.sdx, sdy: p.sdy, ex: p.ex, ey: p.ey, edx: p.edx, edy: p.edy });
for (let k = 0; k < 160; k++) {
  const big = k % 10 === 9;
  const leaves = (big ? 14 : 2) + (next() % (big ? 16 : 10));
  const groupCount = k % 8 === 0 ? 0 : 1 + (next() % (big ? 6 : 4));
  const mixed = k % 3 === 0;
  const n = leaves + groupCount;
  const isGroup = Array.from({ length: n }, () => false);
  const groupIds = [];
  while (groupIds.length < groupCount) {
    const i = next() % n;
    if (!isGroup[i]) {
      isGroup[i] = true;
      groupIds.push(i);
    }
  }
  const nodes = [];
  for (let i = 0; i < n; i++) {
    let c;
    if (isGroup[i]) {
      c = reference.cnode(0, 0);
      c.isGroup = true;
      c.dir = mixed && next() % 2 === 0 ? pick(DIRS) : undefined;
      c.padX = k % 4 === 1 ? 12 : 20;
      c.padTop = 20 + (next() % 3) * 12;
      c.padBottom = c.padX;
      c.minW = next() % 3 === 0 ? 40 + (next() % 160) : 0;
      c.span = Infinity;
    } else {
      c = reference.cnode(30 + (next() % 100), 20 + (next() % 45));
      c.span = next() % 4 === 0 ? 0 : Math.max(0, c.w - 24);
    }
    c.seq = k % 6 === 5 ? next() % n : i;
    nodes.push(c);
  }
  for (let i = 0; i < n; i++) {
    if (groupIds.length === 0 || next() % 3 === 0) continue;
    const g = pick(groupIds);
    if (g === i) continue;
    nodes[i].parent = g;
  }
  if (k % 25 === 7 && k % 4 !== 2 && groupIds.length > 0) nodes[0].parent = n + 3;
  const edges = [];
  const edgeCount = 1 + (next() % (big ? 3 * n : 2 * n));
  for (let j = 0; j < edgeCount; j++) {
    const src = next() % n;
    let dst = next() % n;
    if (src === dst && next() % 4 !== 0) dst = (dst + 1) % n;
    const labelled = next() % 4 === 0;
    edges.push({
      src,
      dst,
      minlen: next() % 6 === 0 ? 2 : 1,
      labelW: labelled ? 20 + (next() % 60) : 0,
      labelH: labelled ? 22 : 0,
      route: [],
      labelX: 0,
      labelY: 0,
    });
  }
  if (k % 3 === 0) {
    const at = next() % n;
    edges.push({ src: at, dst: at, minlen: 1, labelW: k % 2 ? 0 : 30, labelH: k % 2 ? 0 : 22, route: [], labelX: 0, labelY: 0 });
  }
  const rootDir = pick(DIRS);
  const inherit = k % 4 === 2;
  const opt = {
    nodeSep: k % 5 === 4 ? 20 : 40,
    edgeSep: 16,
    rankSep: k % 5 === 4 ? 36 : 48,
    portSep: 20,
    tracks: k % 5 === 1,
    straight: k % 7 === 3,
    headRoom: 7,
  };
  const initial = { nodes: structuredClone(nodes), edges: structuredClone(edges), rootDir, opt, inherit };
  if (inherit) reference.inheritCrossedDirections(nodes, edges.map((e) => [e.src, e.dst]));
  const result = reference.compoundLayout(nodes, edges, rootDir, opt);
  const placed = () => ({
    nodes: nodes.map(({ x, y, w, h, parent, seq, dir }) => ({ x, y, w, h, parent, seq, dir })),
    edges: edges.map(({ src, dst, route, labelX, labelY }) => ({ src, dst, route: route.slice(), labelX, labelY })),
  });
  const expected = { result, ...placed() };
  let wrap;
  if (k % 3 === 1) {
    const width = Math.max(1, result.width * (0.3 + (next() % 5) / 10));
    const gap = 40;
    const wrapped = reference.wrapComponents(nodes, edges, width, gap);
    wrap = { width, gap, result: wrapped ?? null, ...(wrapped ? placed() : {}) };
  }
  cases.push({ ...initial, expected, wrap });
  if (k % 2 === 0) {
    const curve = CURVES[k % CURVES.length];
    const fills = nodes
      .filter((c) => c.isGroup)
      .map((c, i) => ({ box: [c.x - c.w / 2, c.y - c.h / 2, c.x + c.w / 2, c.y + c.h / 2], shape: ` style="fill:${i % 2 ? '#202020' : '#f0f0f0'};"` }));
    const line = k % 8 === 0 ? ' style="stroke:#f00;stroke-width:2px;"' : '';
    const crossings = new Map();
    for (const e of edges) reference.markCrossings(crossings, e.route);
    const titles = nodes.map((c, i) => {
      if (!c.isGroup) return null;
      const width = 20 + ((i * 37) % 90);
      const x = reference.clusterTitleX(c, width, crossings, 20);
      return { index: i, width, x, crossed: reference.titleCrossed(c, width, x, crossings) };
    }).filter(Boolean);
    const drawn = edges.map((e, i) => {
      const start = MARKERS[(i + k) % 4];
      const end = MARKERS[(i * 3 + k + 1) % 4];
      const startTrim = reference.markerTrim(start);
      const endTrim = reference.markerTrim(end);
      if (e.src === e.dst) {
        const copy = { ...e };
        const c = nodes[e.src];
        const path = reference.loopPath(c, c.w * 0.8, c.h * 0.8, i % 3, k % 2 === 1, 26, endTrim, copy);
        return { loop: true, k: i % 3, sideways: k % 2 === 1, end, path: pathOf(path), labelX: copy.labelX, labelY: copy.labelY,
          over: reference.overFills(path.d, [path.sx, path.sy, path.ex, path.ey], fills, line) ?? null };
      }
      if (e.route.length < 6) return null;
      const reach = i % 3 === 0 ? [startTrim + 9, endTrim + 9] : [undefined, undefined];
      const path = reference.routePath(e.route, curve, startTrim, endTrim, ...reach);
      return {
        route: e.route, start, end, reach: reach[0] === undefined ? null : reach, path: pathOf(path),
        markers: reference.marker(start, path.sx, path.sy, path.sdx, path.sdy, 'C', 'S') + reference.marker(end, path.ex, path.ey, path.edx, path.edy, 'C'),
        over: reference.overFills(path.d, [path.sx, path.sy, path.ex, path.ey], fills, line, path.tokens) ?? null,
        casing: reference.casing(path.d, '', line, i % 5 === 0),
      };
    });
    paths.push({ case: k, curve: curve ?? null, line, fills, titles, edges: drawn });
  }
}
const replacer = (_key, value) => (typeof value === 'number' && !Number.isFinite(value) ? String(value) : value);
const destination = fileURLToPath(new URL('../tests/fixtures/compound-reference.json', import.meta.url));
fs.writeFileSync(destination, JSON.stringify({ upstreamRevision: revision, seed, cases }, replacer) + '\n');
console.log(`Wrote ${cases.length} compound reference cases to ${destination}`);
const edgeDestination = fileURLToPath(new URL('../tests/fixtures/edge-reference.json', import.meta.url));
fs.writeFileSync(edgeDestination, JSON.stringify({ upstreamRevision: revision, seed, cases: paths }, replacer) + '\n');
console.log(`Wrote ${paths.length} edge drawing cases to ${edgeDestination}`);
