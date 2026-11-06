import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Terminal = (await import(pathToFileURL(path.join(artifacts, 'terminal.mjs')))).default;
const oracle = JSON.parse(fs.readFileSync(path.join(artifacts, 'terminal-plan-oracle.json'), 'utf8'));
assert.equal(oracle.reference, 'zz_terminal::TerminalViewport::apply_patch');

function id(high, low) {
  return {$: 'Id', high, low};
}

function list(values) {
  return values.reduceRight((tail, head) => ({$: 'Con', head, tail}), {$: 'Nil'});
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

function scalars(state) {
  return [state.generation.high, state.generation.low, state.view_generation.high,
    state.view_generation.low, state.dictionary.generation, state.grid.columns,
    state.grid.rows, state.dictionary.styles, state.dictionary.graphemes, state.dictionary.bytes];
}

let accepted = 0;
const requests = [];
for (const vector of oracle.vectors) {
  assert.equal(vector.header.length, 29);
  const initial = viewport(vector.header);
  const incoming = patch(vector.header, vector.spans);
  const result = Terminal.transition(initial, incoming);
  assert.equal(result.code, vector.status, JSON.stringify(vector));
  assert.deepEqual(scalars(result.viewport), vector.after, vector.name);
  accepted += Number(vector.status === 0);
  requests.push(['terminal', ...vector.header, ...vector.spans.flat()].join(' '));
}

const native = spawnSync('python3', ['-c',
  'import subprocess,sys; sys.exit(subprocess.run([sys.argv[1], "serve"], input=sys.stdin.buffer.read()).returncode)',
  path.join(artifacts, 'zz-bend')], {
  input: requests.join('\n') + '\n', encoding: 'utf8', timeout: 30000, maxBuffer: 8 * 1024 * 1024,
});
assert.equal(native.status, 0, native.stderr || native.error?.message);
const responses = native.stdout.trim().split('\n').map(Number);
assert.equal(responses.length, oracle.vectors.length);
for (let i = 0; i < responses.length; ++i) {
  assert.equal(responses[i], oracle.vectors[i].status, requests[i]);
}
const report = {reference: oracle.reference, cases: oracle.vectors.length, accepted,
  rejected: oracle.vectors.length - accepted, native: responses.length, result: 'all exact comparisons passed'};
fs.writeFileSync(path.join(artifacts, 'terminal-plan-differential.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
