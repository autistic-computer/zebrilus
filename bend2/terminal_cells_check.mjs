import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Cells = (await import(pathToFileURL(path.join(artifacts, 'terminal_cells.mjs')))).default;
const oracle = JSON.parse(fs.readFileSync(path.join(artifacts, 'terminal-cells-oracle.json'), 'utf8'));
assert.equal(oracle.reference, 'zz_terminal::TerminalViewport::apply_patch.cells');

function id(high, low) {
  return {$: 'Id', high, low};
}

function list(values) {
  return values.reduceRight((tail, head) => ({$: 'Con', head, tail}), {$: 'Nil'});
}

function packed(values) {
  return list(values.map(([glyph, style_flags]) => ({$: 'Cell', glyph, style_flags})));
}

function unpacked(cells) {
  const result = [];
  while (cells.$ === 'Con') {
    result.push([cells.head.glyph, cells.head.style_flags]);
    cells = cells.tail;
  }
  assert.equal(cells.$, 'Nil');
  return result;
}

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

let accepted = 0;
let compared = 0;
for (const vector of oracle.vectors) {
  const current = packed(vector.current);
  const changed = packed(vector.changed);
  const view = viewport(vector.header);
  const incoming = patch(vector.header, vector.spans);
  const result = Cells.apply(current, changed, view, incoming);
  assert.equal(result.code, vector.status, vector.name);
  assert.deepEqual(unpacked(result.cells), vector.after, vector.name);
  accepted += Number(vector.status === 0);
  compared += vector.after.length;
  if (vector.status === 0 && vector.current.length > 0) {
    const truncated = Cells.apply(current.tail, changed, view, incoming);
    assert.equal(truncated.code, 4);
    assert.deepEqual(unpacked(truncated.cells), vector.current.slice(1));
  }
}
const payload = oracle.vectors.find(vector => vector.status === 0 && vector.changed.length > 0);
const shortChanged = Cells.apply(packed(payload.current), packed(payload.changed).tail,
  viewport(payload.header), patch(payload.header, payload.spans));
assert.equal(shortChanged.code, 4);
assert.deepEqual(unpacked(shortChanged.cells), payload.current);
const report = {reference: oracle.reference, cases: oracle.vectors.length, accepted,
  rejected: oracle.vectors.length - accepted, cells_compared: compared, result: 'all exact cell comparisons passed'};
fs.writeFileSync(path.join(artifacts, 'terminal-cells-differential.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
