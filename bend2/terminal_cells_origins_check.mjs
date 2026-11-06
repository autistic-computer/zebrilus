import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Origins = (await import(pathToFileURL(path.join(artifacts, 'terminal_cells_origins.mjs')))).default;
const oracle = JSON.parse(fs.readFileSync(path.join(artifacts, 'terminal-cells-oracle.json'), 'utf8'));
assert.equal(oracle.reference, 'zz_terminal::TerminalViewport::apply_patch.cells');

const id = (high, low) => ({$: 'Id', high, low});
const list = values => values.reduceRight((tail, head) => ({$: 'Con', head, tail}), {$: 'Nil'});
const packed = values => list(values.map(([glyph, style_flags]) => ({$: 'Cell', glyph, style_flags})));

function viewport(h) {
  return {$: 'Viewport', generation: id(h[0], h[1]), view_generation: id(h[2], h[3]),
    grid: {$: 'Grid', columns: h[5], rows: h[6]},
    dictionary: {$: 'Dictionary', generation: h[4], styles: h[7], graphemes: h[8], bytes: h[9]}};
}

function patch(h, spans) {
  return {$: 'Patch', base: id(h[10], h[11]), base_view: id(h[12], h[13]),
    generation: id(h[14], h[15]), view_generation: id(h[16], h[17]),
    dictionary_generation: h[18], grid: {$: 'Grid', columns: h[19], rows: h[20]},
    style_base: h[21], styles: h[22], grapheme_base: h[23], graphemes: h[24], bytes: h[25],
    positive: h[26] !== 0, shift: h[27], cells: h[28],
    spans: list(spans.map(([row, start, length, clear]) => ({$: 'Span', row, start, length, clear: clear !== 0})))};
}

function resolved(origins, current, changed) {
  const output = [];
  while (origins.$ === 'Con') {
    const origin = origins.head;
    if (origin.$ === 'Cleared') {
      output.push([0, 0]);
    } else {
      assert.ok(Number.isSafeInteger(origin.index) && origin.index >= 0);
      const source = origin.$ === 'Retained' ? current : origin.$ === 'Incoming' ? changed : null;
      assert.ok(source && origin.index < source.length);
      output.push(source[origin.index]);
    }
    origins = origins.tail;
  }
  assert.equal(origins.$, 'Nil');
  return output;
}

function unpacked(cells) {
  const output = [];
  while (cells.$ === 'Con') {
    output.push([cells.head.glyph, cells.head.style_flags]);
    cells = cells.tail;
  }
  assert.equal(cells.$, 'Nil');
  return output;
}

let compared = 0;
let accepted = 0;
let bendResolverCases = 0;
let truncated = 0;
for (const row of oracle.vectors) {
  const view = viewport(row.header);
  const incoming = patch(row.header, row.spans);
  const plan = Origins.plan(row.current.length, row.changed.length, view, incoming);
  assert.equal(plan.code, row.status, row.name);
  assert.equal(Origins.all_in_bounds(plan.origins, row.current.length, row.changed.length, true), true, row.name);
  assert.deepEqual(resolved(plan.origins, row.current, row.changed), row.after, row.name);
  if (row.current.length <= 45) {
    const actual = Origins.resolve_outcome(plan, {$: 'Sources', retained: packed(row.current), changed: packed(row.changed)});
    assert.equal(actual.code, row.status, row.name);
    assert.deepEqual(unpacked(actual.cells), row.after, row.name);
    bendResolverCases++;
  }
  compared += row.after.length;
  accepted += Number(row.status === 0);
  if (row.status === 0 && row.current.length > 0) {
    const current = row.current.slice(1);
    const rejected = Origins.plan(current.length, row.changed.length, view, incoming);
    assert.equal(rejected.code, 4, row.name);
    assert.deepEqual(resolved(rejected.origins, current, row.changed), current, row.name);
    truncated++;
  }
}
const payload = oracle.vectors.find(row => row.status === 0 && row.changed.length > 0);
const changed = payload.changed.slice(1);
const rejected = Origins.plan(payload.current.length, changed.length,
  viewport(payload.header), patch(payload.header, payload.spans));
assert.equal(rejected.code, 4);
assert.deepEqual(resolved(rejected.origins, payload.current, changed), payload.current);
truncated++;
const report = {reference: oracle.reference, cases: oracle.vectors.length, accepted,
  rejected: oracle.vectors.length - accepted, source_indices_checked: compared,
  full_cells_compared: compared, bend_resolver_cases: bendResolverCases,
  atomic_truncated_payload_cases: truncated,
  result: 'all bounded source-index plans resolve to exact source cells and rejection outcomes'};
fs.writeFileSync(path.join(artifacts, 'terminal-cells-origins-differential.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
