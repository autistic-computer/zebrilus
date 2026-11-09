import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const artifacts = path.join(root, 'target', 'port');
const Core = (await import(pathToFileURL(path.join(artifacts, 'core.mjs')))).default;
const Edit = (await import(pathToFileURL(path.join(artifacts, 'layout_edit.mjs')))).default;
const Replay = (await import(pathToFileURL(path.join(artifacts, 'replay.mjs')))).default;
const oracle = JSON.parse(fs.readFileSync(path.join(artifacts, 'oracle.json'), 'utf8'));
assert.equal(oracle.protocol_version, 120);

for (const vector of oracle.envelopes) {
  assert.equal(Core.status_code(Core.validate_envelope(...vector.args)), vector.status,
    `envelope ${JSON.stringify(vector.args)}`);
}

function listValues(list) {
  const values = [];
  while (list.$ === 'Con') {
    values.push(list.head);
    list = list.tail;
  }
  assert.equal(list.$, 'Nil');
  return values;
}

for (const vector of oracle.layouts) {
  assert.deepEqual(Core.swapped_layout(vector.node, vector.source, vector.target), vector.swapped);
  assert.deepEqual(Core.swapped_layout(vector.swapped, vector.source, vector.target), vector.node);
  assert.equal(Core.pane_count(vector.node), vector.panes.length);
  assert.equal(Core.contains(vector.node, vector.source), vector.contains);
  assert.deepEqual(listValues(Core.panes(vector.node)), vector.panes);
  assert.deepEqual(Core.shape(vector.node), Core.shape(vector.swapped));
  for (const join of vector.joins) {
    const result = Edit.joined_layout(vector.node, vector.source, vector.target,
      join.split, join.axis, join.ratio, join.before);
    assert.deepEqual(result.$ === 'Some' ? result.value : null, join.result);
  }
}

const phases = ['PhaseStarting', 'PhaseReady', 'PhaseRunning', 'PhaseAwaitingPermission', 'PhaseFailed'];
for (const vector of oracle.attention) {
  assert.equal(Core.attention({ $: phases[vector.phase] }, vector.permission).$, vector.status);
}

function bendList(values) {
  return values.reduceRight((tail, head) => ({$: 'Con', head, tail}), {$: 'Nil'});
}

for (const vector of oracle.replay) {
  const input = structuredClone(vector.initial);
  input.journal = bendList(input.journal);
  const result = Replay.apply(bendList(vector.items), input);
  const actual = result.cursor;
  actual.journal = listValues(actual.journal);
  assert.deepEqual(actual, vector.cursor, JSON.stringify(vector));
  assert.deepEqual(result.request.$ === 'Some' ? result.request.value : null, vector.request);
  assert.equal(result.invalid, vector.invalid);
}

const native = path.join(artifacts, 'zz-bend');
for (const vector of oracle.envelopes.filter((_, index) => index % 83 === 0)) {
  const output = spawnSync(native, ['validate', ...vector.args.map(String)], {encoding: 'utf8'});
  assert.equal(output.status, 0, output.stderr);
  assert.equal(Number(output.stdout.trim()), vector.status);
}
const malformed = spawnSync(native, ['validate', 'not-a-number'], {encoding: 'utf8'});
assert.equal(malformed.status, 64);

console.log(JSON.stringify({
  envelopes: oracle.envelopes.length,
  layouts: oracle.layouts.length,
  attention: oracle.attention.length,
  replay: oracle.replay.length,
  joins: oracle.layouts.reduce((sum, layout) => sum + layout.joins.length, 0),
  native: oracle.envelopes.filter((_, index) => index % 83 === 0).length,
  result: 'all exact comparisons passed'
}));
